#!/usr/bin/env python3
"""authored SDNA layouts, not Blender-generated. Values define topology."""
import struct,json,hashlib
from pathlib import Path
pack=lambda f,*v:struct.pack('<'+f,*v)
def make(case):
    S={'Opaque':[('char','byte')],'ID':[('char','name[66]'),('Library','*lib')],'Library':[('ID','id'),('char','name[128]')],'ListBase':[('void','*first'),('void','*last')],'bNodeTree':[('ID','id'),('ListBase','nodes')],'bNode':[('bNode','*next'),('bNode','*prev'),('char','idname[64]'),('int','type'),('ListBase','inputs'),('ListBase','outputs'),('void','*storage')],'Inner':[('float','scale')],'Storage':[('int','operation'),('char','_pad[4]')]}
    if case=='nested':S['Storage'].append(('Inner','options'))
    if case=='path':S['Storage'].append(('char','filepath[128]'))
    if case=='nested-path':S['Inner'].append(('char','directory[128]'));S['Storage'].append(('Inner','options'))
    if case=='pointer':S['Storage'].append(('ID','*dependency'))
    if case=='label':S['Storage'].append(('char','label[128]'))
    socket=case.startswith('socket')
    if socket:
        S['bNodeSocket']=[('bNodeSocket','*next'),('bNodeSocket','*prev'),('char','idname[64]'),('int','type'),('void','*default_value')]
        S['Tex']=[('ID','id'),('int','type')]
        S['SocketValue']=[('Tex','*value')]
        if case=='socket-void':S['SocketValue'].append(('void','*opaque'))
        if case=='socket-path':S['SocketValue'].append(('char','filepath[128]'))
    L={'char':1,'void':0,'int':4,'float':4}
    def parts(n):
        p=n.startswith('*');n=n.lstrip('*');a=n.split('[');return a[0],p,int(a[1][:-1]) if len(a)>1 else 1
    def size(t):
        if t not in L:L[t]=sum((8 if parts(n)[1] else size(ty))*parts(n)[2] for ty,n in S[t])
        return L[t]
    for t in S:size(t)
    T=list(L);N=list(dict.fromkeys(n for fs in S.values() for ty,n in fs));ST=list(S)
    def record(t,values):
        b=b''
        for ty,n in S[t]:
            name,ptr,count=parts(n);v=values.get(name,0 if ptr or ty in ('int','float') else b'' if ty=='char' else {})
            if ptr:b+=pack('Q',v)
            elif ty=='char':b+=v.ljust(count,b'\0')
            elif ty=='int':b+=pack('i',v)
            elif ty=='float':b+=pack('f',v)
            else:b+=record(ty,v)
        assert len(b)==L[t];return b
    def block(code,ty,address,body):return pack('4siQii',code,len(body),address,ST.index(ty) if ty else 0,1)+body
    d=b'SDNANAME'+pack('I',len(N))+b'\0'.join(n.encode() for n in N)+b'\0';d+=bytes(-len(d)%4)
    d+=b'TYPE'+pack('I',len(T))+b'\0'.join(t.encode() for t in T)+b'\0';d+=bytes(-len(d)%4)
    d+=b'TLEN'+pack('H'*len(T),*(L[t] for t in T));d+=bytes(-len(d)%4);d+=b'STRC'+pack('I',len(S))
    for t,fields in S.items():
        d+=pack('HH',T.index(t),len(fields))
        for ty,n in fields:d+=pack('HH',T.index(ty),N.index(n))
    b=b'BLENDER-v405'+block(b'NT\0\0','bNodeTree',0x100,record('bNodeTree',{'id':{'name':b'NTstorage'},'nodes':{'first':0x200,'last':0x200}}))
    b+=block(b'DATA','bNode',0x200,record('bNode',{'idname':b'GeometryNodeLegacyAttributeRandomize','storage':0x300,'inputs':{'first':0x400 if socket else 0,'last':0x400 if socket else 0}}))
    b+=block(b'DATA','Storage',0x300,record('Storage',{'filepath':b'//potential.png','options':{'directory':b'//potential/'},'dependency':0x100,'label':b'//also-potential.png'}))
    if socket:
        b+=block(b'DATA','bNodeSocket',0x400,record('bNodeSocket',{'idname':b'NodeSocketTexture','default_value':0x500}))
        b+=block(b'DATA','SocketValue',0x500,record('SocketValue',{'value':0x700,'opaque':0xdead,'filepath':b'//unknown.png'})+(b'\0' if case=='socket-extent' else b''))
        b+=block(b'LI\0\0','Library',0x800,record('Library',{'id':{'name':b'LItexture'},'name':b'//socket-texture.blend'}))
        b+=block(b'ID\0\0','ID',0x700,record('ID',{'name':b'TEtexture','lib':0x800}))
    ending=block(b'DNA1',None,0,d)+pack('4siQii',b'ENDB',0,0,0,0)
    linked=b'BLENDER-v405'+block(b'LI\0\0','Library',0x1000,record('Library',{'id':{'name':b'LIstorage'},'name':('//'+case+'.blend').encode()}))
    linked+=block(b'ID\0\0','ID',0x1100,record('ID',{'name':b'NTstorage','lib':0x1000}))
    if socket:
        leaf=b'BLENDER-v405'+block(b'TE\0\0','Tex',0x100,record('Tex',{'id':{'name':b'TEtexture'}}))+ending
        (root/'socket-texture.blend').write_bytes(leaf)
    return b+ending,linked+ending
root=Path(__file__).resolve().parent;rows=[]
for case in ('numeric','nested','path','nested-path','pointer','label','socket','socket-extent','socket-void','socket-path'):
    body,linked=make(case)
    for name,b in ((case+'.blend',body),(case+'-root.blend',linked)):
        (root/name).write_bytes(b);rows.append({'path':name,'sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b)})
b=(root/'socket-texture.blend').read_bytes();rows.append({'path':'socket-texture.blend','sha256':hashlib.sha256(b).hexdigest(),'bytes':len(b)})
(root/'manifest.json').write_text(json.dumps({'author':'oxblend','Blender_generated':False,'files':rows},indent=2)+'\n')
