# Zamil: create native saved Text flags without running any Text content.
import bpy,json,sys
from pathlib import Path
E=Path(sys.argv[sys.argv.index('--')+1]);E.mkdir()
bpy.ops.wm.read_factory_settings(use_empty=True)
external_path=E/'external.txt';external_path.write_text('EXTERNAL_ORIGINAL\n')
external=bpy.data.texts.load(str(external_path),internal=False);external.name='External'
dirty=bpy.data.texts.load(str(external_path),internal=False);dirty.name='ExternalDirty';dirty.write('EXTERNAL_DIRTY_EDIT\n')
embedded=bpy.data.texts.new('Embedded');embedded.write('EMBEDDED_ORIGINAL\n');embedded.filepath='R:\\old-workstation\\historical.py'
internal_path=E/'internal.txt';internal_path.write_text('INTERNAL_LOADED\n')
internal=bpy.data.texts.load(str(internal_path),internal=True);internal.name='LoadedInternal'
def facts():return [dict(name=t.name,filepath=t.filepath,is_in_memory=t.is_in_memory,is_dirty=t.is_dirty,body=t.as_string()) for t in bpy.data.texts]
(E/'created.json').write_text(json.dumps(dict(version=bpy.app.version_string,texts=facts()),indent=2))
bpy.ops.wm.save_as_mainfile(filepath=str(E/'texts.blend'),compress=False)
