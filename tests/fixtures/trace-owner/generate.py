# Small serialized namespace contracts, not Blender-certified files.
# Literal widths/addresses/counts define fixture topology, not resource limits.
from pathlib import Path
import struct,hashlib,json
S={
 'ID':[('char','name[66]'),('char','*lib'),('char','*override_library')],
 'ListBase':[('char','*first'),('char','*last')],
 'Object':[('ID','id'),('char','*adt'),('int','totcol'),('char','*mat'),('char','*data'),('int','transflag'),('char','*dup_group'),('char','*proxy'),('char','*proxy_group'),('char','*pose'),('ListBase','particlesystem'),('ListBase','modifiers')],
 'ModifierData':[('char','*next'),('char','*prev'),('int','type')],
 # Native MeshCacheModifierData uses FILE_MAX=1024; this is a schema width,
 # not a runtime budget. Small128 made legitimate relocated paths TMPDIR-sensitive.
 'MeshCacheModifierData':[('ModifierData','modifier'),('char','filepath[1024]')],
 'IDOverrideLibrary':[('char','*reference'),('char','*hierarchy_root'),('int','flag'),('ListBase','properties')],
 'IDOverrideLibraryProperty':[('char','*next'),('char','*prev'),('char','*rna_path'),('int','tag'),('int','rna_prop_type'),('ListBase','operations')],
}
L={'char':1,'int':4}
def parts(n):
 p=n.startswith('*');n=n.lstrip('*');a=n.split('[');return a[0],p,int(a[1][:-1]) if len(a)>1 else 1

def size(t):
 if t not in L:L[t]=sum((8 if parts(n)[1] else size(ty))*parts(n)[2] for ty,n in S[t])
 return L[t]
for t in S:size(t)
T=list(L);N=list(dict.fromkeys(n for fs in S.values() for ty,n in fs));names=list(S)
pack=lambda f,*a:struct.pack('<'+f,*a)
def dna():
 b=b'SDNANAME'+pack('I',len(N))+b'\0'.join(n.encode() for n in N)+b'\0';b+=b'\0'*(-len(b)%4)
 b+=b'TYPE'+pack('I',len(T))+b'\0'.join(t.encode() for t in T)+b'\0';b+=b'\0'*(-len(b)%4)
 b+=b'TLEN'+pack('H'*len(T),*(L[t] for t in T));b+=b'\0'*(-len(b)%4);b+=b'STRC'+pack('I',len(S))
 for t,fs in S.items():
  b+=pack('HH',T.index(t),len(fs))
  for ty,n in fs:b+=pack('HH',T.index(ty),N.index(n))
 return b

def record(t,v):
 b=b''
 for ty,n in S[t]:
  key,p,count=parts(n);x=v.get(key,0 if p or ty=='int' else b'' if ty=='char' else {})
  if p:b+=pack('Q',x)
  elif ty=='char':b+=x.ljust(count,b'\0')
  elif ty=='int':b+=pack('i',x)
  else:b+=record(ty,x)
 assert len(b)==L[t];return b

def head(code,ty,addr,data):return pack('4siQii',code,len(data),addr,names.index(ty) if ty else 0,1)+data

def fixture(mode):
 b=b'BLENDER-v405'
 for i,label in [(1,b'A'),(2,b'B')]:
  b+=head(b'OB','Object',i,record('Object',{'id':{'name':b'OB'+label,'override_library':0x400},'modifiers':{'first':0x200,'last':0x200}}))
  if mode!='foreign-only' or i!=1:
   m=head(b'DATA','MeshCacheModifierData',0x200,record('MeshCacheModifierData',{'modifier':{'type':46},'filepath':b'//'+label+b'.pc2'}));b+=m
   if mode=='same-id' and i==1:b+=m
  b+=head(b'DATA','IDOverrideLibrary',0x400,record('IDOverrideLibrary',{'reference':2 if i==1 else 0,'properties':{'first':0x500,'last':0x500}}))
  b+=head(b'DATA','IDOverrideLibraryProperty',0x500,record('IDOverrideLibraryProperty',{'rna_path':0x600}))
  b+=head(b'DATA',None,0x600,b'property_'+label+b'\0')
 return b+head(b'DNA1',None,0,dna())+pack('4siQii',b'ENDB',0,0,0,0)
D=Path(__file__).resolve().parent;manifest={}
for mode in ['cross-id','same-id','foreign-only']:
 p=D/(mode+'.blend');p.write_bytes(fixture(mode));manifest[p.name]={'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()}
(D/'manifest.json').write_text(json.dumps({'author':'oxblend','scope':'native owner grammar; synthetic serialized branch contracts only','files':manifest},indent=2)+'\n')
