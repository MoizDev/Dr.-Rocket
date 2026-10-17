# dmgbuild settings for the Dr. Rocket disk image: a branded window with the app on the
# left, an arrow, and Applications on the right. Used by tools/bundle.sh --dmg.
#
#   dmgbuild -s tools/dmg/settings.py -D app="target/Dr. Rocket.app" "Dr. Rocket" out.dmg
#
# background.tiff holds the 1x and 2x art, rendered from background.html (640x420) with
# headless Chrome and joined with `tiffutil -cathidpicheck bg.png bg@2x.png -out ...`.
import os.path

app = defines["app"]  # noqa: F821 (dmgbuild provides `defines`)
name = os.path.basename(app)
here = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else "tools/dmg"

format = "UDZO"
filesystem = "HFS+"
files = [app]
symlinks = {"Applications": "/Applications"}
background = os.path.join(here, "background.tiff")

window_rect = ((200, 160), (640, 420))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
icon_size = 128
text_size = 13
icon_locations = {name: (170, 205), "Applications": (470, 205)}
hide_extensions = [name]
