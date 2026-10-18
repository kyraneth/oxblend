# Serialized read/admission contracts, not native-certified scenes.
# Reuse the existing schema builder without re-running or replacing its fixtures.
from pathlib import Path
import hashlib,json
parent=Path(__file__).resolve().parent.parent
exec((parent/'generate.py').read_text().split('D=Path(__file__)')[0])
D=Path(__file__).resolve().parent
cross=(parent/'cross-id.blend').read_bytes()
files={}
unknown=bytearray(cross);unknown[12:16]=b'ZZ\0\0';files['unknown-id.blend']=bytes(unknown)
ignored=head(b'DATA',None,0x7000,b'ignored\0')
files['data-before-id.blend']=cross[:12]+ignored+cross[12:]
# A top-level TEST control closes the prior ID's contiguous DATA run.
control=head(b'TEST',None,0,b'')
obj2=cross.index(b'OB\0\0',12+4)
files['data-after-control.blend']=cross[:obj2]+control+ignored+cross[obj2:]
# ID is struct zero in this schema. Blender read_struct accepts a raw SDNA=0
# linked-ID placeholder; it still participates in the global ID map.
S['Library']=[('ID','id'),('char','name[256]'),('char','filepath[256]')]
L={'char':1,'int':4}
for t in S:size(t)
T=list(L);N=list(dict.fromkeys(n for fs in S.values() for ty,n in fs));names=list(S)
linked=(b'BLENDER-v405'+head(b'LI','Library',0x1000,record('Library',{'id':{'name':b'LIlibrary'},'name':b'//library.blend','filepath':b'//library.blend'}))
 +head(b'ID','ID',0x2000,record('ID',{'name':b'MElinked','lib':0x1000}))
 +head(b'OB','Object',0x3000,record('Object',{'id':{'name':b'OBobject'},'data':0x2000}))
 +head(b'DNA1',None,0,dna())+pack('4siQii',b'ENDB',0,0,0,0))
files['raw-linked-id.blend']=linked
# A linked placeholder returns before read_data_into_datamap. Subsequent DATA
# is skipped, not admitted to the placeholder's owner scope.
placeholder_end=12+24+L['Library']+24+L['ID']
files['data-after-placeholder.blend']=linked[:placeholder_end]+ignored+linked[placeholder_end:]
manifest={}
for name,body in files.items():
 (D/name).write_bytes(body);manifest[name]={'bytes':len(body),'sha256':hashlib.sha256(body).hexdigest()}
(D/'manifest.json').write_text(json.dumps({'author':'oxblend','scope':'portable serialized read/admission contracts; no native receiving claim','files':manifest},indent=2)+'\n')
