import sys,struct
def f(h): return struct.unpack('<f',struct.pack('<I',int(h,16)))[0]
for l in open(sys.argv[1]):
    t=l.split()
    if t[0]=='F':
        d=dict(); i=1
        print(' '.join(t[:9]), 'cam', [round(f(x),4) for x in t[10:13]], 'fac',[round(f(x),4) for x in t[14:17]], 'nfac',[round(f(x),4) for x in t[18:21]],'ovl',t[26],'player',[f(x) for x in t[28:30]])
    elif t[0]=='E':
        g=lambda k,n=3:[round(f(x),5) for x in t[t.index(k)+1:t.index(k)+1+n]]
        print(' E%s t%s time%s world%s v%s a%s rotz%.5f scl%s col %s col2 %s fl %s I %s %s D%s' % (t[1],t[3],t[7],g('world'),g('v'),g('a'),f(t[t.index('rot')+3]),g('scl',2),t[t.index('col')+1],t[t.index('col2')+1],t[t.index('fl')+1],t[t.index('I')+1],t[t.index('I')+2],g('D')))
