"""Opens a prop of the project in Blender to edit it: the editor's "Edit in Blender" runs

    blender --python tools/blender/open_prop.py -- PROJECT PROP_ID

The add-on is registered, the prop imported (its model, or its stand-in rebuilt from its
shapes, with its light) and framed, and an edit session starts: the prop is written back to
the project by itself, a moment after each change and on every Ctrl+S (the .blend lives in
build/blender/), into the space it came from, its record's facts and asset-browser folders
kept. The editor sees the change within a second.
"""
import os
import sys
from pathlib import Path

import bpy

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
project = Path(argv[0]).resolve() if argv else Path.cwd()
prop_id = argv[1] if len(argv) > 1 else ""
os.environ["BATTLE_FRONTIER_ROOT"] = str(project)
sys.path.insert(0, str(project / "tools" / "blender"))
import battle_frontier as bf  # noqa: E402

try:
    bf.unregister()
except Exception:  # noqa: BLE001 - not registered yet
    pass
bf.register()
bf.remember(str(project))

for o in list(bpy.data.objects):
    bpy.data.objects.remove(o, do_unlink=True)
if prop_id:
    if bpy.ops.battle_frontier.import_prop(prop_id=prop_id) == {"FINISHED"}:
        bf.start_editing(bpy.context, prop_id)


def frame():
    """The view on the prop, the add-on's tab open, once the window is up."""
    for window in bpy.context.window_manager.windows:
        for area in window.screen.areas:
            if area.type != "VIEW_3D":
                continue
            for region in area.regions:
                if region.type == "WINDOW":
                    with bpy.context.temp_override(window=window, area=area, region=region):
                        bpy.ops.view3d.view_all(center=True)
                    for space in area.spaces:
                        if space.type == "VIEW_3D":
                            space.show_region_ui = True
                    return None
    return 0.5


if not bpy.app.background:
    bpy.app.timers.register(frame, first_interval=0.5)
