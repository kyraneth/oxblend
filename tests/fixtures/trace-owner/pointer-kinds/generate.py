# Crafted serialized namespace contracts; not native Blender saves.
# Synthetic alias topology plus explicit typed/mirror variants. Literal addresses
# describe fixture identities, not budgets or limits.
from pathlib import Path
import struct,json,hashlib
D=Path(__file__).resolve().parent
g={};exec((D.parent/'generate.py').read_text().split('D=Path(__file__)')[0],g)
g['S']['Mesh']=[('ID','id')];g['size']('Mesh')
# The char* Object.data pointer uses the ID field role, not the DATA run.
g['T']=list(g['L']);g['N']=list(dict.fromkeys(n for fs in g['S'].values() for ty,n in fs));g['names']=list(g['S'])
head,record,dna=g['head'],g['record'],g['dna'];pack=struct.pack
def build(mesh=True,typed=False,mirror=False):
 b=b'BLENDER-v405'
 for i,label in [(1,b'A'),(2,b'B')]:
  base=0x1000*i
  values={'id':{'name':b'OB'+label,'override_library':0x900 if mirror and i==1 else base+0x400},'data':base+0x200 if i==1 else 0,'modifiers':{'first':0x900 if mirror and i==1 else base+0x200,'last':0x900 if mirror and i==1 else base+0x200}}
  if typed:values['mesh']=0x1200 if i==1 else 0;values['data_ptr']=0x900 if i==1 else 0
  b+=head(b'OB','Object',i,record('Object',values))
  b+=head(b'DATA','MeshCacheModifierData',base+0x200,record('MeshCacheModifierData',{'modifier':{'type':46},'filepath':b'//'+label+b'.pc2'}))
  b+=head(b'DATA','IDOverrideLibrary',base+0x400,record('IDOverrideLibrary',{'reference':2 if i==1 else 0,'properties':{'first':base+0x500,'last':base+0x500}}))
  b+=head(b'DATA','IDOverrideLibraryProperty',base+0x500,record('IDOverrideLibraryProperty',{'rna_path':base+0x600}))
  b+=head(b'DATA',None,base+0x600,b'property_'+label+b'\0')
 if mesh:b+=head(b'ME','Mesh',0x900 if mirror else 0x1200,record('Mesh',{'id':{'name':b'MEalias'}}))
 return b+head(b'DNA1',None,0,dna())+pack('<4siQii',b'ENDB',0,0,0,0)
original=build();assert original==(D.parent/'id-data-alias.blend').read_bytes()
outputs={'owner-id-alias.blend':original,'data-only-id.blend':build(mirror=True),'id-only-data.blend':build(mesh=False)}
g['S']['Object'] += [('Mesh','*mesh'),('IDOverrideLibrary','*data_ptr')]
g['L']['Object']+=16
g['T']=list(g['L']);g['N']=list(dict.fromkeys(n for fs in g['S'].values() for ty,n in fs));g['names']=list(g['S'])
outputs.update({'typed-id-data-alias.blend':build(typed=True),'typed-id-only-data.blend':build(mesh=False,typed=True),'typed-data-only-id.blend':build(typed=True,mirror=True)})
# Native Material** first reads owner DATA; each entry then lib-links a Material ID.
g['S']['Material']=[('ID','id'),('char','*adt')];g['size']('Material')
g['S']['Object']=[('Material','**mat') if n=='*mat' else (ty,n) for ty,n in g['S']['Object']]
g['T']=list(g['L']);g['N']=list(dict.fromkeys(n for fs in g['S'].values() for ty,n in fs));g['names']=list(g['S'])
b=b'BLENDER-v405'
for i,label in [(1,b'A'),(2,b'B')]:
 base=0x1000*i
 b+=head(b'OB','Object',i,record('Object',{'id':{'name':b'OB'+label},'totcol':1 if i==1 else 0,'mat':0x950 if i==1 else 0,'modifiers':{'first':base+0x200,'last':base+0x200}}))
 b+=head(b'DATA','MeshCacheModifierData',base+0x200,record('MeshCacheModifierData',{'modifier':{'type':46},'filepath':b'//'+label+b'.pc2'}))
 if i==1:b+=head(b'DATA',None,0x950,pack('<Q',0x1200))
# Global IDs collide with both the outer pointer-table and inner DATA addresses.
for addr,label in [(0x950,b'outer'),(0x1200,b'inner')]:b+=head(b'MA','Material',addr,record('Material',{'id':{'name':b'MA'+label}}))
outputs['material-pointer-table.blend']=b+head(b'DNA1',None,0,dna())+pack('<4siQii',b'ENDB',0,0,0,0)
# A linked selection visits only OBA, so inner-hop ID-vs-DATA selection has a
# visible consequence: its Material ID references a deliberately missing library.
g['S']={'Pad':[('char','pad')],**g['S']};g['size']('Pad')
g['S']['Library']=[('ID','id'),('char','name[1024]'),('char','filepath[1024]')];g['size']('Library')
g['T']=list(g['L']);g['N']=list(dict.fromkeys(n for fs in g['S'].values() for ty,n in fs));g['names']=list(g['S'])
def end(body):return body+head(b'DNA1',None,0,dna())+pack('<4siQii',b'ENDB',0,0,0,0)
child=b'BLENDER-v405'+head(b'OB','Object',1,record('Object',{'id':{'name':b'OBA'},'totcol':1,'mat':0x950,'modifiers':{'first':0x1200,'last':0x1200}}))
child+=head(b'DATA','MeshCacheModifierData',0x1200,record('MeshCacheModifierData',{'modifier':{'type':46},'filepath':b'//A.pc2'}))+head(b'DATA',None,0x950,pack('<Q',0x1200))
child+=head(b'MA','Material',0x950,record('Material',{'id':{'name':b'MAouter'}}))+head(b'ID','ID',0x1200,record('ID',{'name':b'MAinner','lib':0x980}))
missing=b'//missing-material-library.blend'
child+=head(b'LI','Library',0x980,record('Library',{'id':{'name':b'LImissing'},'name':missing,'filepath':missing}))
outputs['selected-material-child.blend']=end(child)
childpath=b'//selected-material-child.blend'
parent=b'BLENDER-v405'+head(b'ID','ID',1,record('ID',{'name':b'OBA','lib':0x990}))+head(b'LI','Library',0x990,record('Library',{'id':{'name':b'LIchild'},'name':childpath,'filepath':childpath}))
outputs['selected-material-root.blend']=end(parent)
manifest={}
for name,body in outputs.items():
 (D/name).write_bytes(body);manifest[name]={'bytes':len(body),'sha256':hashlib.sha256(body).hexdigest()}
(D/'manifest.json').write_text(json.dumps({'author':'oxblend','scope':'Synthetic serialization contracts, not native qualification or templates','files':manifest},indent=2)+'\n')
