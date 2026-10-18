#!/usr/bin/env python3
"""authored SDNA topology, including integrity seeds; not native Blender."""
import struct,json,hashlib
from pathlib import Path
P=lambda f,*v:struct.pack('<'+f,*v)
root=Path(__file__).resolve().parent

def generate(case):
 S={'Opaque':[('char','byte')],'ID':[('char','name[66]'),('Library','*lib')],'Library':[('ID','id'),('char','name[128]')],'ListBase':[('void','*first'),('void','*last')],'Object':[('ID','id'),('ListBase','constraints')],'bAction':[('ID','id')],'bConstraint':[('bConstraint','*next'),('bConstraint','*prev'),('short','type'),('void','*data')],'ConstraintData':[('Object','*tar'),('bAction','*act'),('char','subtarget[64]'),('ChannelDriver','*driver'),('ListBase','targets')],'ChannelDriver':[('ListBase','variables')],'DriverVar':[('DriverVar','*next'),('DriverVar','*prev'),('char','name[64]')],'Target':[('Target','*next'),('Target','*prev'),('Object','*tar')],'OtherTarget':[('OtherTarget','*next'),('OtherTarget','*prev'),('Object','*tar')]}
 if case=='void':S['ConstraintData'].append(('void','*opaque'))
 if case=='path':S['ConstraintData'].append(('char','filepath[128]'))
 L={'char':1,'void':0,'short':2}
 def parts(n):
  a=n.lstrip('*').split('[');return a[0],n.startswith('*'),int(a[1][:-1]) if len(a)>1 else 1
 def size(t):
  if t not in L:L[t]=sum((8 if parts(n)[1] else size(ty))*parts(n)[2] for ty,n in S[t])
  return L[t]
 for t in S:size(t)
 T=list(L);ST=list(S);N=list(dict.fromkeys(n for fields in S.values() for ty,n in fields))
 def record(t,values):
  b=b''
  for ty,n in S[t]:
   name,ptr,count=parts(n);v=values.get(name,0 if ptr or ty=='short' else b'' if ty=='char' else {})
   if ptr:b+=P('Q',v)
   elif ty=='short':b+=P('h',v)
   elif ty=='char':b+=v.ljust(count,b'\0')
   else:b+=record(ty,v)
  assert len(b)==L[t];return b
 def block(code,t,a,values,extra=b''):
  body=record(t,values)+extra;return P('4siQii',code,len(body),a,ST.index(t),1)+body
 d=b'SDNANAME'+P('I',len(N))+b'\0'.join(n.encode() for n in N)+b'\0';d+=bytes(-len(d)%4)
 d+=b'TYPE'+P('I',len(T))+b'\0'.join(t.encode() for t in T)+b'\0';d+=bytes(-len(d)%4)
 d+=b'TLEN'+P('H'*len(T),*(L[t] for t in T));d+=bytes(-len(d)%4);d+=b'STRC'+P('I',len(ST))
 for t,fields in S.items():
  d+=P('HH',T.index(t),len(fields))
  for ty,n in fields:d+=P('HH',T.index(ty),N.index(n))
 end=P('4siQii',b'DNA1',len(d),0,0,1)+d+P('4siQii',b'ENDB',0,0,0,0)
 header=b'BLENDER-v301'
 b=header+block(b'OB\0\0','Object',0x100,{'id':{'name':b'OBowner'},'constraints':{'first':0x200,'last':0x200}})
 b+=block(b'DATA','bConstraint',0x200,{'type':12,'data':0x300})
 b+=block(b'DATA','ConstraintData',0x300,{'tar':0xdead if case=='dangling' else 0x800,'act':0x900,'subtarget':b'Bone','driver':0x400,'targets':{'first':0x600,'last':0x600 if case=='last' else 0x700},'opaque':0xdead,'filepath':b'//unknown.png'},b'\0' if case=='extent' else b'')
 b+=block(b'DATA','ChannelDriver',0x400,{'variables':{'first':0x500,'last':0x500}})
 b+=block(b'DATA','DriverVar',0x500,{'name':b'different SDNA TLEN'},b'\0' if case=='driver-extent' else b'')
 b+=block(b'DATA','Target',0x600,{'next':0x700,'tar':0x800})
 b+=block(b'DATA','OtherTarget' if case=='sibling' else 'Target',0x700,{'prev':0 if case=='reciprocity' else 0x600,'next':0x600 if case=='cycle' else 0,'tar':0x800})
 b+=block(b'LI\0\0','Library',0x1000,{'id':{'name':b'LItar'},'name':b'//tar.blend'})
 b+=block(b'ID\0\0','ID',0x800,{'name':b'OBtarget','lib':0x1000})
 b+=block(b'LI\0\0','Library',0x1100,{'id':{'name':b'LIact'},'name':b'//act.blend'})
 b+=block(b'ID\0\0','ID',0x900,{'name':b'ACmotion','lib':0x1100})
 driver=header+block(b'LI\0\0','Library',0x2000,{'id':{'name':b'LIroot'},'name':('//'+case+'.blend').encode()})+block(b'ID\0\0','ID',0x2100,{'name':b'OBowner','lib':0x2000})+end
 return {case+'.blend':b+end,case+'-root.blend':driver,'tar.blend':header+block(b'OB\0\0','Object',0x100,{'id':{'name':b'OBtarget'}})+end,'act.blend':header+block(b'AC\0\0','bAction',0x100,{'id':{'name':b'ACmotion'}})+end}
files={}
for case in ('normal','extent','driver-extent','void','path','reciprocity','last','cycle','sibling','dangling'):files.update(generate(case))
rows=[]
for name,body in files.items():
 (root/name).write_bytes(body);rows.append({'path':name,'sha256':hashlib.sha256(body).hexdigest(),'bytes':len(body)})
(root/'manifest.json').write_text(json.dumps({'author':'oxblend','Blender_generated':False,'files':rows},indent=2)+'\n')
