"use client";

import { useEffect, useRef } from "react";

/** The Visualizer demo: plays (muted, looping) while on screen, unless motion is reduced. */
export default function VizVideo({ className }: { className?: string }) {
  const ref = useRef<HTMLVideoElement>(null);

  useEffect(() => {
    const video = ref.current;
    if (!video || matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) video.play().catch(() => {});
        else video.pause();
      },
      { threshold: 0.25 },
    );
    io.observe(video);
    return () => io.disconnect();
  }, []);

  return (
    <video
      ref={ref}
      className={className}
      src="/media/visualizer.mp4"
      poster="/media/visualizer-poster.jpg"
      width={1920}
      height={1204}
      muted
      loop
      playsInline
      preload="metadata"
      aria-label="The Visualizer replaying a binary search tree program at 2x: frames for each insert and tree-sum call appear, the tree is drawn node by node, and each step is explained in plain words."
    />
  );
}
