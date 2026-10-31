"""Backend regressions. Run with a full Racket 9.3 runtime; no global installation.
RACKETEER_RACKET=/path/to/racket python3 -m unittest discover -s racket/tests -p '*.py' -v
All programs and generated source live in subprocesses/scratch directories.
"""
import base64
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
RACKET = os.environ.get('RACKETEER_RACKET')


class Backend:
    def __init__(self, script='runner.rkt'):
        self.process = subprocess.Popen([RACKET, str(ROOT / 'racket' / script)],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, cwd=ROOT)
        self.queue = queue.Queue(16)
        self.errors = []
        self.max_line = 0
        self.chunks = 0
        def output():
            for line in self.process.stdout:
                self.queue.put(line)
            self.queue.put(None)
        def errors():
            for line in self.process.stderr:
                self.errors.append(line.decode(errors='replace'))
                self.errors[:] = self.errors[-20:]
        self.readers = [threading.Thread(target=f, daemon=True) for f in (output, errors)]
        for reader in self.readers:
            reader.start()

    def send(self, **message):
        self.process.stdin.write((json.dumps(message) + '\n').encode())
        self.process.stdin.flush()

    def run(self, source, instrument='none', memory=None):
        self.send(cmd='run', path=None, source=source, offset=0,
                  language={'kind': 'module'},
                  options={'instrument': instrument, 'submodules': [],
                           'memory_limit_mb': memory, 'print_width': 80})

    def next(self, timeout=40):
        payload = bytearray()
        deadline = time.monotonic() + timeout
        while True:
            line = self.queue.get(timeout=max(.001, deadline - time.monotonic()))
            if line is None:
                raise EOFError(''.join(self.errors))
            self.max_line = max(self.max_line, len(line))
            if len(line) > 4 * 1024 * 1024:
                raise AssertionError('wire line exceeded 4 MiB')
            message = json.loads(line)
            if 'chunk' not in message:
                if payload:
                    raise AssertionError('interleaved chunks')
                return message
            self.chunks += 1
            payload.extend(base64.b64decode(message['chunk']))
            if len(payload) > 64 * 1024 * 1024:
                raise AssertionError('logical response exceeded 64 MiB')
            if message['last']:
                return json.loads(payload)

    def until(self, predicate, timeout=45):
        deadline = time.monotonic() + timeout
        events = []
        while True:
            event = self.next(max(.001, deadline - time.monotonic()))
            events.append(event)
            if predicate(event):
                return events

    def close(self):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=5)
        self.process.stdin.close()
        # Drain a backpressured reader before joining it.
        while any(reader.is_alive() for reader in self.readers):
            try:
                self.queue.get(timeout=.1)
            except queue.Empty:
                pass
        self.process.stdout.close()
        self.process.stderr.close()


@unittest.skipUnless(RACKET, 'set RACKETEER_RACKET to a full Racket 9.3 runtime')
class Regressions(unittest.TestCase):
    def backend(self, script='runner.rkt'):
        backend = Backend(script)
        self.addCleanup(backend.close)
        return backend

    def test_ready_follows_logs_from_definitions_and_repeated_interactions(self):
        b = self.backend()
        b.run('#lang racket\n'
              '(for ([i (in-range 40)]) (log-warning "initial ~a" i))\n'
              '(define-logger app)\n(log-app-error "last initial")\n')
        events = b.until(lambda e: e.get('ev') == 'ready')
        logs = [e['message'] for e in events if e.get('ev') == 'log']
        self.assertEqual(logs, [f'initial {i}' for i in range(40)] + ['app: last initial'])
        for ident in range(1, 11):
            b.send(cmd='eval', id=ident,
                   source=f'(log-warning "first {ident}") (log-app-error "last {ident}")')
            events = b.until(lambda e: e.get('ev') == 'ready' and e.get('id') == ident)
            logs = [e['message'] for e in events if e.get('ev') == 'log']
            self.assertEqual(logs, [f'first {ident}', f'app: last {ident}'])

    def test_exit_emits_once_and_terminates_for_all_exit_values(self):
        for value, code in [('3', 3), ('300', 0), ('-1', 0), ('#f', 0), ('"bye"', 0)]:
            with self.subTest(value=value):
                b = self.backend()
                b.run(f'#lang racket/base\n(display "bye")\n(exit {value})')
                events = b.until(lambda e: e.get('ev') == 'exit')
                self.assertEqual(events[-1]['code'], code)
                self.assertEqual(''.join(e.get('text', '') for e in events), 'bye')
                self.assertEqual(b.process.wait(timeout=5), code)
                rest = []
                while True:
                    try:
                        rest.append(b.next(2))
                    except EOFError:
                        break
                self.assertFalse(any(e.get('ev') == 'exit' for e in rest))

    def test_deep_error_and_large_value_keep_session_usable(self):
        b = self.backend()
        b.run('''#lang racket/base
(define (f n) (if (zero? n) (error 'boom "x") (+ 1 (f (sub1 n)))))
(f 5000)''', 'errortrace')
        events = b.until(lambda e: e.get('ev') == 'ready')
        error = next(e for e in events if e.get('ev') == 'error')
        self.assertLessEqual(len(error['stack']), 100)
        self.assertGreater(len(error['stack']), 1)
        b.send(cmd='eval', id=1, source='(build-list 250000 values)')
        events = b.until(lambda e: e.get('ev') == 'ready')
        value = next(e for e in events if e.get('ev') == 'value')
        self.assertNotIn('text', value)  # no duplicate wire representation
        text = ''.join(s.get('text', '') for s in value['segments'])
        self.assertIn('[value truncated]', text)
        self.assertLess(len(text), 270000)
        b.send(cmd='eval', id=2, source='(+ 1 2)')
        self.assertTrue(any(e.get('segments') == [{'text': '3'}]
                            for e in b.until(lambda e: e.get('ev') == 'ready')))

    def test_expansion_error_recovers_language_and_runtime_error_keeps_definitions(self):
        for source, expression, expected in [
            ('#lang htdp/bsl\nunbound-name', '(+ 1 2)', '3'),
            ('#lang htdp/bsl\n(define x', '(+ 1 2)', '3'),
            ('#lang racket/base\n(', '(+ 1 2)', '3'),
            ('#lang racket/base\n(define x 42)\n(error "boom")', 'x', '42'),
            ('#lang racket/base\nunbound-name', '(+ 1 2)', '3')]:
            with self.subTest(source=source):
                b = self.backend()
                b.run(source)
                b.until(lambda e: e.get('ev') == 'ready')
                b.send(cmd='eval', id=1, source=expression)
                events = b.until(lambda e: e.get('ev') == 'ready')
                self.assertFalse(any(e.get('ev') == 'error' for e in events), events)
                self.assertTrue(any(e.get('segments') == [{'text': expected}] for e in events), events)

    def test_output_memory_pause_resume_repeatedly_preserves_order(self):
        for attempt in range(10):
            with self.subTest(attempt=attempt):
                b = self.backend()
                b.run('''#lang racket/base
(define xs (for/list ([i 80]) (displayln i) (sleep 0.002) (make-bytes 1048576)))
(displayln "done")''', memory=32)
                events = b.until(lambda e: e.get('ev') == 'memory-limit', timeout=15)
                self.assertIsNone(b.process.poll())
                b.send(cmd='memory-continue', limit_mb=512)
                events += b.until(lambda e: e.get('ev') == 'ready', timeout=15)
                output = ''.join(e.get('text', '') for e in events if e.get('ev') == 'output')
                self.assertEqual(output.splitlines(), [str(i) for i in range(80)] + ['done'])
                b.close()

    def test_large_check_response_is_chunked_and_server_survives_next_check(self):
        b = self.backend('server.rkt')
        source = '#lang racket\n' + ''.join(
            f'(define (f{i} x) (let ([a (+ x 1)]) (+ a a)))\n' for i in range(2500))
        b.send(cmd='check-syntax', id=1, path=None, source=source, offset=0)
        result = b.next(90)
        self.assertTrue(result['ok'], result.get('error'))
        self.assertGreater(len(json.dumps(result).encode()), 4 * 1024 * 1024)
        self.assertGreater(b.chunks, 1)
        self.assertGreater(len(result['result']['arrows']), 2500)
        self.assertTrue(any(c['style'] == 'lexically-bound' for c in result['result']['colors']))
        b.send(cmd='check-syntax', id=2, path=None, source='#lang racket/base\n(+ 1 2)', offset=0)
        self.assertEqual(b.next()['id'], 2)
        b.send(cmd='bluebox-cached', id=3, cache_id=result['result']['cache_id'], pos=source.find('+'))
        cached = b.next()
        self.assertTrue(cached['ok'], cached.get('error'))
        self.assertIsNotNone(cached['result'])
        b.send(cmd='bluebox-cached', id=4, cache_id=999999, pos=0)
        self.assertIn('cache-miss', b.next()['error'])
        self.assertIsNone(b.process.poll())

    def test_visualizer_caps_then_tail_loop_finishes_and_errors_still_signal_end(self):
        for ending in ['0', '(error "finished with error")']:
            with self.subTest(ending=ending):
                b = self.backend()
                b.run(f'''#lang racket/base
(define (loop n) (if (zero? n) {ending} (loop (sub1 n))))
(loop 1000000)''', 'visualize')
                events = b.until(lambda e: e.get('ev') == 'ready', timeout=20)
                end = next(e for e in events if e.get('ev') == 'viz-end')
                self.assertTrue(end['truncated'])
                self.assertLessEqual(end['steps'], 1500)
                self.assertEqual(any(e.get('ev') == 'error' for e in events), ending != '0')
                self.assertEqual(sum(e.get('ev') == 'viz-end' for e in events), 1)

    def test_visualizer_bounds_values_and_delta_frames_are_replayable(self):
        b = self.backend()
        b.run('''#lang racket
(define s (make-string 100000 #\\a))
(define big (expt 2 100000))
(define complex-big (make-rectangular 1 (/ 1 big)))
(define (walk n s) (if (zero? n) big (+ 1 (walk (sub1 n) s))))
(walk 50 s)''', 'visualize')
        events = b.until(lambda e: e.get('ev') == 'ready')
        frames = []
        delta_saved = 0
        for e in events:
            if e.get('ev') != 'viz-step':
                continue
            prefix = e['frame_prefix']
            self.assertLessEqual(prefix, len(frames))
            delta_saved += prefix
            frames = frames[:prefix] + e['frames']
            self.assertLessEqual(len(frames), 64)
            self.assertLess(len(json.dumps(e)), 1024 * 1024)
        self.assertGreater(delta_saved, 100)
        self.assertTrue(any(e.get('ev') == 'viz-end' and not e['truncated'] for e in events))


if __name__ == '__main__':
    unittest.main()
