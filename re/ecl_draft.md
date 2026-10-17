# ECL working notes (draft; merge into 07_ecl.md)
## File
- hdr: u32 0x800 magic/version (checked); +4 i16 nsubs; +6 i16 ntimelines (runner iterates i<ntl); +8 u32[16] timeline offs (ntl+1 nonzero; last one unused?); +0x48 u32[nsubs] sub offs. Loader relocates the 16 tl offs + sub offs.
- instr: +0 i32 time, +4 i16 op, +6 i16 size, +8 u8 ?, +9 u8 rank mask, +10 u16 param(var) mask, +12 args. Sub ends with op -1 (never executed; just pad; op1 = delete returns -1).
- Timeline instr 0x20 for op0: i16 time, i16 arg0?, i16 op, u8 size, u8 rankmask? ... (verify with runner FUN_0042a8a0)
## VM
- Enemy e (Ecl_Run 0x4184b0 this=file, param e). ctx = e+0x7f8 main (0x228 bytes); ctx: +0 ip, +4 Timer time, +0x10 callback?, +0x18..0x34 int vars (10000-10007), +0x58..0x64 (10036-10039), +0x90 Timer wait, +0x9c 8 interp slots x0x30, +0x220 ?, +0x224 i16 sub id.
- call stack base e+0xa20 (0x228 per frame); depth e+0x2cea (max 15) (0x2ce8 saved main depth). Extra contexts e+0x3384[4] (ptr; ctx at +8, stack +0x230, depth at +6).
- Per instr loop: final_pos(0x2d88)=pos(0x2d34)+offset(0x2d40); if wait>0: wait--,time--, POST. if time.cur != ip.time -> POST (EQUALITY). rank: (ip[9] & (g_rank 0x160f53c | e[0x3330])) must == mask else skip. dispatch op-1 (ops 1..184, others nop).
- interrupt: e+0x2d30 pending idx (>=0) -> sub id from e+0x2cf0[idx] (i16); pushes ctx (unless e+0x3324 bit26) after advancing ip by size, depth++.
- POST: if e+0x2dfc>0 movement interps (8 slots), time.Tick, then next context... finally FUN_00422c40(e), FUN_00423150(e).
## Variables (Ecl_GetIntArg 0x41f420 / Ecl_GetFloatArg 0x420120 / IntVarRef 0x41fe10 / FloatVarRef 0x420950)
ctx=current context (e+0x2ca0 ptr). int read of float var = trunc; float read of int var = (float).
10000-10007 ctx+0x18..0x34 int (RW int)   | 10008-10015 e+0x2ca8..0x2cc4 int (RW)  [per-enemy, shared across contexts]
10016-10023 ctx+0x38..0x54 float (RW f)    | 10024-10031 e+0x2cc8..0x2ce4 float (RW)
10032 rand u32&0x7fffffff | 10033 Rng_NextFloat [0,1) | 10034 rand u32 (as signed int->float) | 10035 Rng_Next32/2^31? -1 (FUN_43ed80: u32/[4b5b14] - 1)
10036-10039 ctx+0x58..0x64 int (RW) | 10040 g 0x160f538 (RW int) | 10041 g 0x164d334 (RW int)
10042-10044 e+0x2d34..3c pos x,y,z (float RW) | 10045-10047 player pos 0x17d61ac.. (float RW!)
10048 angle from enemy final pos to player (0x44c1b0: atan2(py-ey, px-ex); if both 0 -> const [4b4468]) RO
10049 e+0x2e1c int RW | 10050 dist enemy->player RO | 10051 e+0x2dfc int RW (movement-interp flag?) | 10052 g 0x164d0b1 (byte) RO
10053-10056 ctx+0x70..0x7c int RW | 10057-10060 ctx+0x80..0x8c float RW | 10061-10064 global ints 0x4ece20..2c RW | 10065-10068 global floats 0x4ece30..3c RW
10069 e+0x2d94 f | 10070 e+0x2d98 | 10071-73 e+0x2da8..b0 | 10074-76 e+0x2dd0..d8 | 10077 e+0x2d9c | 10078 e+0x2da0 | 10079-81 e+0x2dc4..cc (float RW; int read only up to 10078!)
10082 rand angle: Rng_FloatRange(2pi)-pi (float only) | 10083 e+0x3354 int RO | 10084 e+0x3313 byte RO | 10085-87 e+0x2d64..6c float RO
10088-10091 e+0x3358..3364 int RO | 10092 e+0x3304 int RW | 10093 e+0x2e08 int RW | 10094-95 ctx+0x68,0x6c float RW
10096 child-chain depth (boss/parent links e+0x2da4, e+8) | 10097 byte 0x17d5ef8+5 | 10098 (int only) 2 if (a+b+c < d) else 0 ... (0x160f508 / 0x4ea670 / 0x1653648 counters) | 10099 spellcard state | 10100 (int only) timer 0x4ea670+0x108
Unknown ids: int read -> literal; float read -> literal; refs -> literal slot in script (self-modifying).
## Ops (case = op)
1 delete (return -1) | 2 wait(i n): ctx.wait=n | 4 jump(time, rel_off) | 5 loop(time, rel_off, V counter): counter--, if >0 jump
6 iset(V,i) 7 fset(W,f) 8 irand_sign(V,i): Next16&1 ? +i : -i | 9 frand_sign(W,f) same (bit0=1 -> +)
10 iadd 11 isub 12 imul 13 idiv 14 imod (a op= b) | 15 fadd 16 fsub 17 fmul 18 fdiv 19 fmod(a=fmod(a,b))
20-24 int a=b op c (+,-,*,/,%) | 25-29 float a=b op c (fmod(b,c)) | 30 inc 31 dec
32 fsin(a=sin b) 33 fcos | 34 atan2(W a,f x1,y1,x2,y2): a=atan2(y2-y1,x2-x1) | 35 FUN_421300 36 FUN_4213f0 (TODO)
37 norm_angle(W a) | 38 polar(W x, W y, f ang, f r): x=cos(wrap ang)*r, y=sin*r | 39 dist(W a, x1,y1,x2,y2)
40-51 cmp-jump (a,b,time,rel_off) pairs i/f: 40/41 ==, 42/43 !=, 44/45 <, 46/47 <=, 48/49 >, 50/51 >=  (FUN_4215f0)
52 call(sub): push ctx(ip=next), set sub (time0 wait0), ctx+0x70..0x8c = globals 0x4ece20..3c (10061-68 -> 10053-60); depth++ (<=15) unless e+0x3324 bit26 (no push)
53 return: depth--; <0 => extra thread ends (free ctx, back to main, run next thread); else pop ctx.
54 anm_script(i): AnmFile_SetScript(file 0xf54e0c slot7 enemy.anm?, e+0xc, s); e+0x3328 &= ~4
55 anm_dir_scripts(i base): FUN_421de0(e, base..base+5); &=~4 | 56 same with 6 explicit ints | 57 FUN_421e50(e); &=~4
58-61 same as 54-57 but file 0xf54e10 (stg enm anm) and e+0x3328 |= 4 | 62 restart anm script e+0x333c from file per bit2
63 set_pos(f x, f y): pos=(x,y,0); FUN_42c180(e) | 64 FUN_420f40 (TODO)
65 set_angle_speed(f ang, f spd): angle(0x2d94)=wrap(ang); speed(0x2da8)=spd; move mode bits12-13 of 0x3324 = 1; 0x2de8=0; timer 0x2ddc=0
66 (i n, ?, f ang, f spd): n<1 -> as 65 ; else FUN_420d10(e) (TODO)
67 FUN_422020 | 68 aim(f off, f spd): angle = wrap(angle_to_player_from_pos + off) speed=spd (mode bits not changed!)
69 (i n, ?, f off, f spd): n<1: aim + mode1 + dur n -> 0x2de8,timer ; else FUN_420d10 (hmm both n<1)
70 set_angvel(f): 0x2d98; mode1 | 71 set_accel(f): 0x2dac; mode1 | 72 (i dur, f a1, f a2, ...) 0x2de8=dur timer, 0x2dd0=a1 ...
## Movement (FUN_422c40 after each context run; FUN_42deb0 move; FUN_42c180 clamp)
e+0x2d34 pos, 0x2d4c velocity, 0x2d58 prevpos, 0x2d64 delta(prev frame), 0x2d94 angle, 0x2d98 angvel, 0x2da8 speed, 0x2dac accel,
0x2d9c orbit angle, 0x2da0 orbit angvel, 0x2db0 radius, 0x2db4 radius vel, 0x2dc4 delta vec(mode2), 0x2dd0 start/center, 0x2ddc Timer, 0x2de8 dur.
mode=(0x3324>>12)&3: 1 polar (angle+=gs*angvel wrap; speed+=gs*accel; vel=polar; dur>0: timer-- ; <=0 -> mode0 (velocity kept))
 2 interp: timer--; t=max(0,1-timer/dur); ease (0x3324>>14)&7 (1 t2,2 t3,3 t4,4 out2,5 out3,6 out4); vel=(start+delta*t)-pos (x neg if bit18); angle=atan2(vel); timer<=0 -> mode0, pos=start+delta, vel=0
 3 orbit: oa=wrap(oa+gs*oav); r+=gs*rv; vel=polar(oa,r)+center-pos (xy only); angle=atan2(vel); dur like mode1
move: delta=pos-prev; prev=pos; pos.x += gs*vel.x (or -= if bit18 mirror); y,z += gs*vel
clamp (bit19, op75 rect xmin,ymin,xmax,ymax at 0x3340..) / op76 clears
FUN_423150 (if life e+0x2dfc>0): auto-fire every e+0x3060 frames (timer 0x3064) -> FUN_422720; direction anims by vel.x vs [4b48d8]/[4b48d4] using scripts e+0x3332..0x333a (0x332e current dir)
72 move_orbit_params(i dur, f cx, f cy, f oa, f oav, f r, f rv): mode3 | 73 (i dur, f oa, f oav, f rv): center=pos, r=0, mode3 | 74 (i dur, f oav, f rv) mode3
75 clamp_rect(f x0,y0,x1,y1) bit19 | 76 clamp off | 77 hitbox(f w,f h)->0x2d70/74 | 78 (f,f)->0x2d7c/80 (kill box?)
79 flags_set_neg(i m): bit6=!(m&1) bit2=!(m&2) bit3=!(m&4) bit4=(m&8) bit28=(m&16) [0x3328 bit6]=(m&32)
80 flags_clear(i m) / 81 flags_set(i m) (see code) | 82 (f r): 0x3350 = r*r | 83 (i b): 0x3328 bit1 = b&1
86 get_int_from_enemy(V a, i var, i slot): a = GetIntArg(enemy[slot] (0xf54cc0[]), var) | 87 same float
## Bullets (ops 96-104 -> FUN_422720 fire, aim mode = op-96)  instr 44 bytes
arg layout: +0xc i16 type (mask bit0), +0xe i16 colour (bit1), +0x10 i32 count1 (bit2, stored i16), +0x14 count2 (bit3), +0x18 f speed1 (bit4), +0x1c f speed2 (bit5), +0x20 f angle1 (bit6), +0x24 f angle2 (bit7), +0x28 u32 flags
- if e.0x3324 bit17 (op107 on / op108 off): instead of firing, copy whole instr to e+0x3034 (autofire template; fired every e.0x3060 frames by FUN_423150)
- only when life(e+0x2dfc)>0. flags&0x8000: only if e bit11 set; flags&0x10000: only if bit11 clear. if e.0x3350 (op82 r^2) >0: only if dist^2(final_pos, player) >= r^2
- desc at e+0x2e24: pos=final_pos+shot_offset(0x2db8, op110); unless spellcard active (0x4ea670 bit0): count1 += lerp_i(e.0x2df4,e.0x2df6) min1; count2 += lerp_i(0x2df8,0x2dfa) min1; if speed1!=0: speed1 += lerp_f(0x2dec,0x2df0) min 0.3; speed2 += lerp_f(...)/2 min 0.3
  lerp_i(a,b) = a + trunc((b-a)*rank/32); lerp_f = a + (b-a)*rank/32.0; rank = int at 0x164d334 (= var 10041)
- BulletMgr_Spawn 0x430e10: if bullet count(0x160f3c8) < 0x600: aim=angle(spawnpos->player); for j<count2: for i<count1: place (stop when no free slot); if flags&0x200 play sfx desc+0x200
- per bullet (0x42f5f0): speed = count2<2 ? s1 : s1-(s1-s2)*j/count2.
  mode0/1: a = (count1 even ? a2*0.5 + (i/2)*a2 : ((i+1)/2)*a2); if i odd a=-a; mode0 += aim; += a1
  mode2: a=aim then mode3: a += j*a2 + a1 + i*2pi/count1 | mode4: a=aim then mode5: a += i*2pi/count1 + pi/count1 + a1
  mode6: a = rand*(a1-a2)+a2 | mode7: speed = rand*(s1-s2)+s2, angle as mode3 (no aim) | mode8: a rand then speed rand
  bullet pos z = 0.1; vel = polar(wrap(a), speed*gamespeed); flags 2/4/8 -> spawn-effect states 2/3/4
88 call_on_enemy(i slot, i sub): FUN_421bd0(enemy[slot], sub) (call in other enemy's ctx) | 89 set_interrupt_on(i slot, i idx): enemy[slot].pending=idx
90 spawn_child_A / 91 spawn_child_B (FUN_41f280) / 92 spawn_child_C (child pos offset = parent pos, bit9 follow parent) -- TODO details FUN_41efc0/41f110/41f280; sfx 0x24
93 (f? a0 raw, f x,y,z, i,i,i) FUN_42a680(0x577f20,...) spawn something at (x,y,z) abs; 94 same with pos relative to e.pos ; 95 FUN_42efb0(0x577f20,8000,0)
105 autofire_interval(i n): e.0x3060 = n + lerp_i(n/5, -n/5); timer 0x3064=0 | 106 same, timer=Rng_Range(interval)
107 autofire_store_on (bit17) | 108 off | 109 laser?/bullet fire from desc e+0x2e24 directly with pos=final+offset (FUN_430e10) TODO | 110 shot_offset(f x, f y): 0x2db8=(x,y,0)
111 bullet_ex(i idx, i a,i b,i c,i d, f e, f f) -> e+0x2e44+idx*0x18 {e,f,c,d,a,b} (copied to bullets +0xdd0) TODO check int->float
112 FUN_415c60(bulletmgr) TODO | 113 sfx(i id, i x): id<0: desc flag 0x200 off; else desc.sfx=id, flag on; desc+0x204=x
114/115 laser(type|colour, f angle, f speed, f a,b,c,d, i e,f,g, raw h,i, raw flags) desc e+0x3070, +0x1f8 = (op==114); ptr -> e+0x3280[e+0x3300]
116 laser_slot(i) | 117 laser_rotate(i slot, f d) | 118 laser_aim(i slot, f off) | 119 laser_offset(i slot, f x,y,z) | 120 laser_alive(i slot)->ctx+0x60 (10038) | 121 laser_cancel(i slot)
122 FUN_421280 123 FUN_4212e0 | 124 sfx(i id) at pos | 125 interrupt_now(i idx) | 126 set_interrupt(i sub, i idx): e.0x2cf0[idx]=sub
127 boss(i slot): slot<0 unregister else register boss slot 0..3 (0xf54cc0[slot]=e), slot0 -> HUD boss mode; flag bit1
128 effect13(raw,raw,raw,raw ...) | 129 (i) bits20-22 (guard: not in spell practice w/ 0x164d0b4 bits7-8) | 130 (i) e.0x2cee (same guard)
131 set_life(i n): life 0x2dfc = max 0x2e00 = 0x2e04 = n; if boss slot 0 registered: reset 8 HUD entries | 132 (i n) Timer e+0x2e14 = n
133 (i idx,i a,i b): e.0x3358[idx]=a, e.0x3368[idx]=b (b skipped when spell-practice guard) | 134 (i a, i b): e.0x3378=a, 0x337c=b (b skipped under guard); timer 0x2e14=0
135 thread(i slot, i sub): free e.0x3384[slot]; sub>=0: alloc 0x24b0 "ECLInt" zeroed; +0=sub; ctx(+8) = sub start; copy cur ctx+0x18..+0x8f (30 dwords) into it
136 call_native(i n): table 0x4c6cb0[n]() | 137 ctx_callback(i n): ctx+0x10 = table[n] (n<0 none), ctx+0x14 = this instr (called each frame in POST)
138 (raw bytes) e.0x3310..0x3312 = arg0 bytes 0..2 | 139 effect(i id, i n, V/i colour): FUN_425430(effmgr,id,pos,n,colour) | 140 effect_dir(i id,i n,i c,f x,y,z) FUN_425650
141 item(i type) at pos (FUN_4400a0 itemmgr 0x1653648) | 142 item_burst(i n): n items at pos+rand offs (x+=rand*[4b443c]-[4b42c8]); type: if power(FUN_422480)<128 first=2 rest=0 else all 1
143 (i) e.0x3304 (var 10092) | 144 (i a,i b) e.0x3308,0x330c | 145 (raw b) bit25 | 146 (i) FUN_41fdf0(ctx.time, n) TODO | 147 std_section(i) -> 0x4ea290
148 (i) FUN_423130(0x160f428, n); 0x164d30c += 1800 | 149 anm_interrupt(i) main VM e+0xc pending=n | 150 (raw i vm, raw i n) VM[1+vm] (e+0x2b0+vm*0x2a4) pending=n
151 (raw b) bit26 no-stack | 152 rank_params(f s_lo, f s_hi, i c1lo, c1hi, c2lo, c2hi) | 153 0x337c = (int)short 0x2cee; timer 0x2e14=0
154 clear laser slots (32 ptrs) | 155 (raw b) bit27; global 0x4ecca8 = 99999990 | 156 (raw b) bit7; 0x332f=2
157 trail(raw byte flags, i n, i ?, i ?): e.0x534c flags, 0x534e count, 0x5350, 0x5352; flags&8 -> FUN_4649a0(anm, e+0xc, e+0x3e14, n/x*2) (afterimages) TODO
158 hp_bar(i idx, i a, i b, i c): HUD FUN_4230e0(idx, a/maxlife, b/maxlife) + FUN_423110(idx, c) (spell life markers?)
159 (i) e.0x332f byte | 160 (i) Timer e+0x5354 = n | 161 (f r) FUN_430d30(bulletmgr, final_pos, r) (cancel bullets in radius?) | 162 FUN_430830(bulletmgr,4)
163 (i) global 0xf54cec | 164 spellcard(i n, f x?,f y?,f z?): FUN_41f0b0(0x4ea670, n); if n==0: FUN_41f040(0x4ea670, a1,a2,a3) TODO
165 (f) e+0x14 = f (main VM rotation.z? e+0xc+0x8) | 166 polar2(W x, W y, f ang, f r): y=sin(ang)*r, x=cos(ang)*r (no wrap; order y first)
167 laser_set_angle(i slot, f a) | 168 item_burst_type1(i n) | 169 (W a): random angle away from player side (TODO consts 4b42c4,4b4888,4b4884,4b4524)
170 laser +0x599 byte | 171 laser +0x560 f | 172 laser +0x558,+0x55c f | 173 (i) bit30 | 174 (i) aura effect 0x20+n at final pos (e+0x53c8)
175 (i) global 0xf54e2c | 176 last-spell flags in 0x164d0b4; bit30 | 177 (i) e.0x2e04 | 178 FUN_4224a0(e) | 179/180 HUD FUN_439007/4390d6 | 181 extend? (gm counter <12: sfx 0x2d, +1, HUD) | 182 (i) 0x3328 bit8 | 183 (i) bit31 | 184 (i) FUN_41f0e0(0x4ea670, n)
35 lerp(W a, f b, f c, f d): a=(b-c)*d+c | 36 interp_var(raw f varid, i dur, i fn, i mode, f a,b,c,d -> stored (int)?) ctx slot (8 at ctx+0x9c, 0x30 each: fn,Timer,dur[4],fnidx[5],mode[6],p[7..10],varid[0xb]); fn table 0x4c6c90: 0-6 ->0x421120, 7 ->0x421180; POST: tick, t=min(1,elapsed/dur), ease(1 t2..6 out4 as ANM), fn(t); done -> slot freed; if varid is 10042/10043/10044 (pos) -> pos change converted to velocity (0x2d4c) + angle, pos restored
55/59 (i base): dir anims = base..base+5 : 0x3332 idle,0x3338 left,0x333a right,0x3334 left->idle,0x3336 right->idle,0x333c op62; cur dir 0x332e=-1 | 56/60: 6 explicit
57/61 sub_anim(i vm 0|1, i script): <0 disable VM[vm] (e+0x2b0+vm*0x2a4); file per 0x3328 bit2 (57 clears bit2 first, 61 sets)
64 move_to(i dur, i mode, f x, f y): delta=(x,y,0)-final_pos, start=pos, mode2 ease=mode&7, vel=0, x mirror
66 (i dur, i mode, f ang, f spd): dur<1 -> mode1 angle/speed (dur 0); else interp: delta=(cos(wrap a)*spd*dur, sin*spd*dur), start=final_pos, mode2
69 same but dur<1 aims (angle += angle_to_player from pos), dur>=1 NOT aimed (quirk, same as 66)
67 move_rand(i dur, i mode, f spd): random angle quadrant toward/away from player side w/ wall bounce (consts) -> dur<1 mode1 else FUN_4222b0
146 ctx.time += n (Timer_Add)
## Timeline (FUN_42a8a0; per timeline: mgr+0x9dcdd0+i*0x10 {Timer, ptr}; i < hdr.ntl; ptr init = tl offset when 0)
instr: i32 time (<0 end), i16 op, u8 size, u8 rankmask (runs if mask & g_rankbits(0x160f53c) != 0), args
loop: ins.time<0 -> tick,return | cur==time -> exec | cur<time -> tick,return | cur>time -> skip ; advance by size
0 spawn(sub,f x,f y,i life,i item,i score) [skip if FUN_42f320(hud) or 0xf54e2c]; 1 = mirrored
2 spawn_randx(sub,f x0,f x1,f y,life,item,score) x=rand*(x1-x0)+x0; 4 mirrored | 3 spawn_randx_full(sub,f y,life,item,score) x=rand*384; 5 mirrored
6 FUN_439810(hud,a0) msg start? | 7 wait while FUN_43587e(hud) (timer-- and stop) | 8 interrupt boss slot a0 with a1 | 9 FUN_406fa0(gm,a0)
10 wait while boss[a0] exists && bit0 | 11 spawn(sub,x,y,life,a4->0x3308,a5->0x330c,score) item=-1; 12 mirrored | 13 wait for sync value a0 in 0xf54e1c[4] (consumes) | 14 fill empty sync slots with a0 | 15 spawn unconditional (non-mirrored) | 16 0x164d0bb=1
## Enemies: mgr 0x577f20 (=EnemyManager this); template enemy at +0 (0x53d0 bytes), 480 slots at +0x53d0 stride 0x53d0. Create FUN_42a4e0 (timeline) / FUN_42a680 (ECL 93/94, copies caller ctx vars 30 dwords):
 find first slot with !(0x3324 bit0); copy template; e.0x2e0c = slot index; bit18 = mirror; life(0x2dfc)=life if>=0; pos=given; set sub on main ctx; Ecl_Run once (time-0); if -1: freed (and mgr+0x9dcef8 = full flag). else: e.0x2e20 = main VM colour; e.0x3304 = item (signed char); score(0x2e08)=score if>=0; 0x2e00=0x2e04=life.
93 enemy_spawn_abs(raw sub, f x,y,z, i life, i item, i score) | 94 same pos relative to e.pos
## Death (in mgr calc when life(0x2dfc)<1 && !0x3328 bit3 && !bit6): 0x3328|=8; FUN_42adb0(e,1) child chain kill w/ drops; gauge FUN_43c0bb(gm, 0x17d5efb? +200 : -200)
 mode=(0x3324>>20)&7 (op129): 0: add score(e.0x2e08), alive=0, aura VM interrupt 3; 1: add score, bit23, clear bits2,3,6; 2: no score; 0/1/2 -> if boss: HUD off+unregister; FUN_42bea0 drops; boss & no spellcard: bulletmgr FUN_430aa0(8000,1)+kill-all bonus -> score+HUD; life=0
   3: life=1, clear 0x3328 bit3, bits20-22=0, HUD off, death effect x3 (byte 0x3310), aura interrupt 3, if 0x17d5ef8==0: timer 0x18b89ec=90, =3; clear bits30,31
 if !bit10: sfx 2/3, effects (0x3310, 0x3311+4), item 7 if FUN_406d10||406d70
 if death sub 0x2cee>=0 (op130): FUN_415c80(e); depth=0; threads reset; FUN_42bc90; bullet desc e+0x2e24 = defaults(0x57ad44, 0x84 dwords); main ctx sub = 0x2cee; 0x2cee=-1  (enemy keeps living)
## Per-enemy calc order (mgr FUN_42c660): skip if !alive. if bit10 (0x3324, "chained-kill/no-update"?): ... else:
 if bit8: FUN_42c420(e); do { r=Ecl_Run; if -1 -> free(FUN_42bcf0); if !bit29: clamp, move, clamp; if bit9 follow parent pos(0x2d40=parent pos); final=pos+0x2d40 (z=0)... offscreen kill check (FUN_4399ac) ...} while(FUN_42b490(e) || (0x3378>=0 && FUN_42b930(e)))
 main VM e+0xc AnmVm_Run (VM.0x1fc short saved/restored via 0x2e20?); sub VMs 0,1 run (script finished -> index -1)
 collision with player (bit2: FUN_42c290 hitbox 0x2d70) ; damage taken (bit6 shootable) FUN_451670 (player shots) ...
## Life thresholds / timeout (after each Ecl_Run; if returns 1 Ecl_Run is re-run SAME frame)
FUN_42b490: 0x3328 bits4-5 cleared; for i<4 with thr[i]>=0: if life < thr: life=thr, 0x2e04=life, main ctx sub=sub[i] (jump; depth=0, threads freed, FUN_415c80), thr=-1, 0x53cc=(timeout-timer)/60 (bonus secs), timeout=-1, bullet desc reset (0x57ad44), autofire 0, chain kill (with drops), every non-boss alive enemy: life=0 and jump to its death sub (0x2cee) if any; boss & 0x17d5ef8==0 -> timer 0x18b89ec=70, flag=3. return 1.
  else warn level by (life-thr): spellcard? (<120:3,<200:2,<300:1) : (<500:3,<1500:2,<2200:1) -> 0x3328 bits4-5 = max. no thresholds: by life: non-boss (spell? <10 : <50 -> 3); boss (spell? <120/300/400 : <600/1600/2400)
FUN_42b930 timeout (only when 0x3378>=0): boss slot0 -> HUD timer (timeout-timer)/60; if timer(0x2e14)>=timeout: 0x53cc=0; largest thr -> life; main sub=0x337c; timeout=-1; 0x337c=death sub; timer=0; if !bit27: spellcard end FUN_42bc50 + bullets cancel FUN_430830(4); boss flag timer; chain kill no drops; non-boss kill; threads freed; desc reset; autofire 0; FUN_415c80; depth 0. return 1
timer 0x2e14 ticks each frame unless 0x160f534 != 0
## Template defaults (FUN_429e00): alive bit0; bits2,3,6 set; bit4,7,1,17,18,19,25 clear; life 1; score 100; hitbox(0x2d70) 24,24,24; vel 0; angle/speed/accel 0; mode0; death sub -1; interrupts pending -1; thresholds -1; timeout -1; dir anims -1; shot offset 0; autofire 0; rank speed range -0.15..0.15 (reset fn 415c80 uses -0.5..0.5 & counts 0); desc sfx 7, desc+0x204=25; no-fire r^2 (0x3350) = 1024; e.0x3330 = 0 (rank mask extra bits; set 0x20 human / 0x40 youkai only by FUN_42c420 when bit8)
## Drops FUN_42bea0(e, flag): item(0x3304) >=0: effect(0x3311+4,n=3)+item(type); ==-1: every 3rd (global ctr 0xf54ce0%3==0) item from byte table 0x4c70d8[ctr2 0..31 cyc] = 00 00 01 00 01 00 00 00 01 01 00 00 01 01 01 00 01 00 01 00 01 00 01 00 01 00 00 01 01 01 00 00 (+effect n=6); <=-2 none.
   then 0x330c extra items (type 0 if power<128 else 1) and 0x3308 extra items type 1, at pos + (rand*128-64, rand*128-64); counts cleared
## rank mask: D = (diff<4 ? 1<<diff : 0xF) | e.0x3330 ; instr runs iff (mask & D) == D. difficulty = 0x160f538 (var 10040)
## flag bits 0x3324: 0 alive, 1 boss, 2 hurts player on contact, 3 takes damage, 4 not drawn & no screen check, 6 hit by shots, 7 ?, 8 human/youkai reactive (FUN_42c420), 9 follows parent, 10 chained (killed with parent), 11 player youkai mode, 12-13 move mode, 14-16 ease, 17 autofire-store, 18 mirror, 19 clamp, 20-22 death mode, 23 ?, 24 has been on screen, 25..27 misc, 26 no call-stack, 28 keep offscreen, 29 skip movement, 30 ?, 31 ?
## Natives (op136 call once / op137 per-frame callback; fastcall ECX=enemy EDX=instr; A1 = instr+0x10)
0 g 0x4e3d28 = I0(ctx+0x18), g 0x4e3d24 = F0(ctx+0x38) | 1 ScreenEffect(3,1,0xffffffff,0,0x15) | 2 bounce: if pos.x<=0 or >=384 vel.x=-vel.x; if vel.y < F7(ctx+0x54): vel.y += F6(ctx+0x50); if pos.y < -64 vel.y=-vel.y; if pos.y>=480 clear bit28; angle=atan2(vel) if changed
3 effects 0x38 (n9,n10) + VM script 0x61 on second; stage callback 0x4ea28c = 0x4235a0 | 4 bullet border warp (zones 124/260/140/276 & 56/328/72/344, center 192,208; scale 67.88/135.76) | 5 effects 9,10 ->+0x350=0; 0x4e4b60=2
6 as 3 with effect 0x3a, script 0x65 | 7 warp variant (zones 56/328/88/360, -32/416/0/448, center 192,224; 224/135.76) | 8 familiar ring sync: children with same ctx+0x60 tag get index (ctx+0x5c), count ctx+0x30, change flag ctx+0x2c; rotate orbit angle toward leader's angle + idx*2pi/n by 0.02*diff
9 laser-like player hit box (player FUN_44a6a0) rect 590x160 / 590x128 (grace 12 frames) | 10 ScreenEffect(3,5,0x40ffffff,0,0x15)+(7,120,190,60,0x15) | 11 like 9 (590x240/590x192 using pos not final)
12 toggle bullets with flag mask I0: state flip sprite +-16, speed from F0/F1 ... ; I1==0 -> children 0x3328|=0x80, interrupts 2 on 0x4e4b68/0x4e4e0c else clear/1 | 13 stage tint 0xffc03030 (FUN_409160) | 14 3-state bullet toggle variant
15 ScreenEffect(7,20,20,20,0x15) | 16 bullets with flag 0x100000 within 64px(4096 sq) of child (ctx+0x60==0) -> child ctx+0x60=60, ctx+0x34 = this ctx+0x34 | 17 ScreenEffect(3,1,...) | 18 game speed = 1/A1 | 19 I0 = spell id (0x164d0b8)
20 effects 0x41, script 99 | 21 warp variant (zones 112/271/128/287 & 33/350/49/366; 158.39/79.19) | 22 spellcard FUN_415d10(0x4ea670, 0x4b495c, e) | 23 FUN_416160(0x4ea670) | 24 I0 = *(0x160f510+0x1c)
25 like 9 (590x288/590x224) | 26 0x160f534 = A1 byte; interrupts 1 (0) / 2 (!=0) on stage VMs (time stop?) | 27 bullets flag 0x100000 -> spawn enemy sub=I2(ctx+0x60) at bullet pos life 800 item -2 score 10 (vars copied), F0=bullet angle; clear flag
28 slow: game speed=1/A1, stage VM interrupts 2, bullets velocity scaled (FUN_423d70), sprites 0x60..0x6f -> 0x6f | 29 restore speed 1 (flag 0x20 if A1>1), bullets unscaled, sprites restored, interrupts 1 | 30 0x18b8a2c = A1 | 31 item at pos: type 5 (or 3 if 0x17d6ed4)
## Spellcard op122 (244 B): +0xc i16 face/portrait param (param_4 to FUN_415f00), +0xe i16 spell id, +0x10 i32 bonus, +0x14 name[0x30]^0xAA, +0x44 owner[0x30]^0xBB, +0x74 comment1[0x40]^0xDD, +0xb4 comment2[0x40]^0xEE (keys VERIFIED in code)
 FUN_4152a0: spell state 0x4ea670 bit0 on; bonus; decay per sec = (bonus - bonus/7)/(timeout/60) (timeout = e.0x3378 from op134, so 134 must precede); timers = timeout; bit27 -> bonus 99999990; cancels bullets (FUN_415c60), stage spell bg VMs, rank params reset, effect 0x27/0x34 ...
