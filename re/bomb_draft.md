# bomb draft (phase 5) — raw notes, merge into 14
Common: bomb objects 128 x 0x16F0 at player+0x1028: +0 state, +4 counter, +8 f speed/orbit r, +0xC f, +0x10 f angle,
+0x14 pos[3], +0x20 centre[3], +0x1A0 vel[3], +0x1B8 AnmVm (player anm file +0xC), +0x440 draw offset[3],
+0x16E8 damage area ptr, +0x16EC cancel area ptr. Timer = player+0xFF4 (T); "adv" = T.cur != T.prev; T==n test.
Bomb tint 0x40BC60(colour EDX): stage tint per channel: t<60: 128 + (c-128)*t/60; t>=dur-60: back to 128 over last 60; else c; alpha 0x80. calls Stage_SetTint 0x409160. colours: Reimu 0x80404040 (LS 0x802020D0), Marisa 0x80404040, MarisaLS?, Yukari 0x80404040/0x802020D0, ...
REIMU bomb 0x40C010 (draw 0x40C820 tint 80404040, orbs Draw2DSnap at pos+off+shake z0):
 f0: Bomb_Start(200,260,0); eff12 ring 0xFF4040FF; 16 orbs script 0x13, angle_i=-pi+i*pi/8, pos=centre=player, r(+8)=0, state1;
   per orb: cancel circle player r96 life200 item6; damage circle orb r64 dmg5 life200 period2 max200 nohitfx. SE 0xD.
 t<40: angle += (even -pi/60, odd +pi/60); pos = centre + (sin a*r, cos a*r); r += 3.2; vel = pos - prev.
 t==40: +0xC=|vel|, +0x10=atan2(vel), +4=0, speed(+8)=8.
 t>=40 state1 & adv: target = homing target (+0xE2AA4) if x>-100 else player; d=target-pos; k=max(1,|d|/(speed/8)); v'=d/k+vel;
   speed=clamp(|v'|,1,10); vel=v'*speed/|v'|; one-frame cancel circle orb r128 (life0,item6);
   impact if dmg dealt>=max(200) or T>=dur-30: areas off; cancel circle PLAYER r64 dr 4.2667 life30; damage circle orb r64 dr12.8 dmg500 life12 period4 max0;
   eff6 x8 at orb; state2; VM interrupt 1; (vel/8 computed into temp - no effect QUIRK); SE 0xF @orb; ScreenEffect(1,16,8,0).
 t>=40 all non-free: pos += vel*gs. state1: areas follow pos. state2: counter+=1 on adv, >29 -> free. VM run.
REIMU LS 0x40C910 (draw 0x40D010 tint 802020D0, all 128 objects):
 f0 same 16 orbs/areas as bomb (Bomb_Start 200,260,LS=1), +0xFF0=0, SE 0xD.
 every frame state1 orbs: angle += ±pi/60 (even -, odd +); pos=centre+(sin,cos)*r; while T<40 r += 2.4 (even)/1.2 (odd); vel=pos-prev;
   when T >= dur-40-i (orb i): areas off; cancel circle PLAYER r64 dr4.2667 life30; damage circle orb r64 dr8.5333 dmg25 life15 period5 max50; eff6 x8; state2; VM int1; SE 0xF @orb; shake(1,16,8,0). (state2 orbs never freed until bomb end; VM still run)
   areas follow orb pos.
 T>=40 and T%20==0: blast object = objects[16 + (+0xFF0)], then +0xFF0 := 1 (QUIRK: 1st blast obj16, rest obj17 re-used); VM script 0x14; state1;
   colour = tbl 0x4C6100[(T/20)%7] = {8FFFFFFF,8F0000FF,8FFF00FF,8FFF0000,8FFFFF00,8F00FF00,8F00FFFF};
   point = homing target if x>-100 else (R*320+32, R*384+32) RNG x then y; eff 0x31 + eff 0x37 colour; cancel circle r64 dr4.2667 life30;
   damage circle r64 dr8.5333 dmg400 life15 period2; SE 0xF; shake(1,16,8,0); flash(3,8,1,colour). Blasts at T=40,60..180.
 objects 16..127: areas follow; VM run; VM end -> free.
DISSOLVE 0x40D100 (draw 0x40D310): f0: Bomb_Start(dur 120 (40 spell practice), inv 200, 0); eff12 ring 0xFF4040FF; ring eff 0x32 fixed slot4 colour FF4040FF:
 VM pos interp slot0 dur 90 (30 practice) mode 5, start (8,64) -> end (128,0); seg 64, radius 8, width 15, waves 6. SE 0xD. speed mult x,y = 0. bosses 0xF54CC0[0..7]: flags A bit 6 cleared.
 draw tint: t<60 0x80404040 else grey g=(t-60)*176/60+64.
ALICE bomb 0x40D430 (draw 0x40D950 = tint 0x80404040 only; the doll is the OPTION 0 sprite: player+0x40C option: pos +0x6B0, rot z +0x414, alpha +0x5FF, state +0x6D4, timer +0x6EC):
 f0: Bomb_Start(210,250,0); SE 0xD; obj0.pos(+0x14 of player+0x1028) = option pos; SE 6.
 t<60: option pos = start + ((192,224)-start)*(t/60)^2; option rot z -= pi/10/frame; one-frame cancel circle r32 item6 + one-frame damage circle r32 dmg40 (life0) at option.
 t>=60: rot 0, pos (192,224); t>=150: option alpha 0. events (on T==n):
  60/64/68/72/76: eff 0x28 colours FFFFFFFF, FFFFD0D0, FFFFB0B0, FFFF8080, FFFF4040;
  90: SE 0xF; eff 0x2A,0x2B,0x2C; cancel circle r1 dr5 life110 item6; damage circle r1 dr5 dmg70 life110 period5; shake(1,16,8,0); flash(3,8,1,8FFFFFFF);
  100/110/120/130: eff 0x2D FFFFFFFF, FFFFD0D0, FFFF8080, FFFF0000;
  150: flash(3,8,1,8FFFFFFF); shake(1,16,8,0); SE 0x19;  209: option state:=1, option timer 0 (respawn doll).
ALICE LS 0x40D970: Bomb_Start(230,280,LS). SE 0xD (no SE 6). start point = PLAYER pos. t<60 same path/rot/areas. t>=60 doll at (192,224), alpha 0 from t>=128. events:
  60..76 eff 0x28 same colours; 120: SE 0xF, eff 0x2A-0x2C at doll, eff 0x2D at (64,96) FF0000F0, (64,352) FFF00000, (320,352) FF00F000, (320,96) FF00F0F0;
  cancel r1 dr5 life110; damage r1 dr5 dmg70 life110 period5 (no shake/flash); 130/140/150/160: eff 0x2D FFFFFFFF/FFFFD0D0/FFFF8080/FFFF0000;
  180: flash(3,8,1,FFFFFFFF), shake(1,16,8,0), SE 0x19; 229: option state 1 + timer 0.
 draw 0x40DEE0: t<90 tint 802020D0; 90<=t<=120: c=(t-90)*208/30, tint A80 R=(c/5-48)&255 G=c+32 B=c+32, then playfield (32,16,416,464) rect white alpha (t-90)*255/30;
  120<t<=220: playfield rect white alpha 0x70 (no tint call); t>220: tint 802020D0.
RING types used by Alice (types 0x2A-0x2D, updates 0x40E040..0x40E2D0; k = 1-(1-t/40)^2, collapses after t=40, scripts 83 life 80):
 0x2A: radius 256k, 64 seg, waves 5, phase 0; t<40 width 8 else amplitude 64k, width += 2/frame.
 0x2B/0x2C: ELLIPSE mode (+0x334=0): radius 256k (x semi-axis), amplitude 128k (y semi-axis), 48 seg, rotation +pi/4 / -pi/4; width 8, t>=40 += 1.5.
 0x2D: plain ring 8 seg radius 192k width 8.
 Ring renderer band mode CORRECTION: vertex = rotate((cos th*(r+-w), sin th*(amp+-w)), +0x330), th step 2pi/n -> rotated ellipse.
MARISA bomb 0x40E3B0 (draw 0x40E610): Bomb_Start(300,350,0); SE 0x13; obj0.pos=player; 5 VMs at player+0x11E0+i*0x2A4 scripts 0x1E..0x22; speed mult x,y=0.2; ScreenEffect(7,16,120,60,120).
 every adv frame with T%4 != 0: cancel rect centre (192, py/2) 384 x py item6 life0; damage rect (px, py/2) 128 x py dmg12 nohitfx; damage rect (192, py/2) 384 x py dmg6 nohitfx.
 RunRange 5 VMs. Draw: tint 0x80404040; beam i: a = i*pi/15 - pi/2 - 2pi/15 (wrap); pos = player + (cos a, sin a)*sprite_w*scale.x/2; rot z = a; Draw2DRot (+shake, z 0).
VM interp helpers: 0x40EC30 pos(slot0: dur, mode, start, end), 0x40EDA0 scale(slot4), 0x40ED50 alpha(slot2: dur,mode,start,end), 0x40ECA0 colour(slot1: dur,mode,startRGB,endRGB).
MARISA LS 0x40E780 (draw 0x40E610 same): Bomb_Start(350,380,LS); SE 0xD + 0x13; VMs scripts 0x23..0x27; shake(7,16,120,60,120); speed mult 0.2; n(+0xFF0)=0.
 adv frames with T%10==0: ring eff 0x35 at player in fixed slot 4+(n%4); n odd -> script 0x5C (92) instead of 88; segments 32, waves 0;
  pos interp 30f mode4 (0,0,0)->(128,0,0) [radius 0->128]; scale interp 30f mode1 (32,0)->(64,0) [width]; alpha 30f mode3 255->0; colour 30f mode0 white->FF0000(red); VM run once; n++; SE 0x11 @player. (eff upd 0x427AE0 frees at alpha 0)
 areas as bomb but wide damage rect dmg 7 (narrow 12).
REMILIA bomb 0x40EE10 (draw 0x40F550 tint 0x80D02020): bats = the 4 options; option k anchor at player+0x6BC+k*0x2F4 (option+0x2B0).
 f0: Bomb_Start(240,290,0); SE 0xD; objects0..3 pos = anchors; SE 6; n=0; speed mult 0.
 t<60: anchors = start + (target-start)*(t/60)^2, targets A0(px-32,py) A1(px,py-32) A2(px,py+32) A3(px+32,py).
 t>=60: T==60: speed mult 2; interrupt 2 to the 4 option VMs. anchors := targets each frame; cancel circle player r96 life0 item6.
  adv & T%10==0: ring 0x35 slot 4+(n%4), 32 seg, waves 4; pos interp 30f mode4 (0,0,0)->(192, R*128 [RNG], 0); scale interp 30f mode1 (64,0)->(64,0); alpha 255->0 mode3; colour white->red; n++; SE 0x11 @player; VM run.
  shot timer (+0xE2AC4) >= 5: cancel rects at player 96x800 and 800x96 life0 item6.
  T==239: 4 options state:=1, timers 0.  No damage areas: damage comes from shot levels 6/7 of ply02as (09 §2.1).
REMILIA LS 0x40F570 (draw 0x40FCB0 tint 0x80F00000): Bomb_Start(280,320,LS); identical to bomb except speed mult 3 at T==60, ring width interp (64,0)->(128,0), options re-init at T==279.
SAKUYA bomb 0x40FCD0 (draw 0x410300 tint 0x80404040; knives Draw2DRot, rot z = angle):
 f0: Bomb_Start(250,290,0); 96 knife objects state 0; speed mult 0.5; eff 0x14 (type 20) at player kept (follows player while T<=60); SE 5.
 20<=T<116 every frame: SE 6 + shake(1,4,1,0); for k in 0..95 when T==(k%48)*2+20 (adv): if knife k already active -> RETURN (abort rest of function this frame);
   activate: VM script 0x16; angle = k*2pi/96 - pi; speed = R+0.5; accel = R*0.1+0.03; spin = Rng_Range16(1)==0 -> always -pi/20 (QUIRK, 1 RNG draw);
   pos = player + polar(angle,24); age 0; cancel circle r32 life500 item6; damage circle r32 dmg20 life500. (2 knives per even frame: k and k+48)
 per active knife: age<30 or age>69: at age==70(adv): if target x>-100 angle = atan2(target-pos); speed := 14. speed += accel; vel = polar(angle,speed).
   30<=age<=69: angle += spin; vel = 0. areas follow; age<120: if damage area dealt(+0x30)>0 -> VM script 0x17, eff 0 colour FFFF80FF, areas off, SE 0x2B @knife; age>=120: areas off.
   pos += vel (no gs); VM run; age tick. Knives stay active until next bomb.
SAKUYA LS 0x4103F0 (draw 0x410AC0 tint 0x80202080 — draws only knives 0..95 QUIRK): Bomb_Start(320,350,LS); 128 knives cleared; speed 0.5; eff 0x14 follows player T<=60; SE 5; shake(1,4,1,0) once at f0.
 20<=T<148 every frame SE 6 (no shake); knife k (0..127) at T==(k%64)*2+20 (abort-return quirk same); script 0x14; angle = wrap(k*2pi/64 - pi); same RNG triple; dmg 30.
 at age 70: retarget as bomb + eff 0x2E (ring type 46: radius += 8/frame, 12 seg, width 32) at knife.
 hit: if k%3==0 flash(3,2,1,0x208080FF); eff 0 FFFF80FF; script 0x15; areas off; SE 0x2B.
YOUMU bomb 0x411B10 (draw 0x412300): Bomb_Start(220,270,0); SE 0xD; speed mult 0; player VM script 0; SE 6; S=start pos (obj0+0x14), P=(px,416) (obj0+0x1A0).
 t<40: player pos = S + (P-S)*k, k=1-(1-t/40)^2.  T==40: eff 0x28 (ring) at player FF8080FF.
 T==70: player y := 32; at (px,224): cancel rect 96x448 item6 life60; damage rect 96x448 dmg300 life10; damage rect dmg80 life60 period5; eff 0x30 colour FF8080FF; flash(3,8,1,EFFFFFFF); SE 0x2A.
 T==80: at px-32 and px+32: cancel rect 96x448 life60, damage rect dmg100 life40 (period 2 left, 3 right), eff 0x30 each; flash(3,8,1,CFFFFFFF).
 T==90: px-64/px+64 same dmg100; flash BFFFFFFF.  T==100: px-96/px+96 dmg80 life40; flash 8FFFFFFF; Q := player pos (obj0+0x1A0).
 120<=T<150: player pos = Q + (S-Q)*k, k=1-(1-(t-120)/30)^2 (return). T==150: speed mult 1.
 ring type 48 (0x30, script 85, upd 0x413070): t<30: radius 192, 48 seg, width 3, amp 0.0001, rot pi/2 (ellipse mode => vertical line 384 long); t>=30: f=((t-30)/30)^4, amp=192f+0.0001, width=80f+3.
 draw: tint 80404040; T>=70: tint 80000030 + playfield white rect alpha 255 (T<100), 255-(T-100)*255/60 (100<=T<160).
YOUMU LS 0x4123D0 (draw 0x412FA0): Bomb_Start(250,300, isLS=0 QUIRK); same dash/return pattern; T==40 eff 0x28 FFFF8080.
 T==70: y:=32; centre rect: cancel life60; damage dmg500 life0; damage dmg100 life60 period5; eff 0x30 FFFF8080; flash EFFFFFFF; SE 0x2A.
 pairs (left period 2, right period 3; cancel 96x448 life60; damage life40; eff 0x30 FFFF8080 each):
  T==80 +-32 dmg100 flash CFFFFFFF; T==90 +-64 dmg100 AFFFFFFF; T==100 +-96 dmg80 8FFFFFFF; T==110 +-128 dmg60 6FFFFFFF; T==120 +-160 dmg50 5FFFFFFF; T==130 +-192 dmg40 5FFFFFFF + Q:=player.
 150<=T<180 return (k on (t-150)/30); T==180 speed 1.
 draw: as bomb but rect colour RED (R FF G0 B0).
YUKARI bomb 0x410C40 (draw 0x4113A0 tint 80404040; its 2 obj0 VMs never get a script -> nothing drawn INFERRED):
 T==0,10,20,30 (bomb start at 0): cancel circle player r100 dr1 life40 item6; damage circle r100 dr1 dmg70 life40 period5;
  boundary effect 0x24 via SpawnFixedParam at player, fixed slot 4/5/6/7, param (angle, 1, 4) angle = pi/4, 3pi/8, pi/2, 5pi/8; slots 5..7 VM script overridden 0x59/0x5A/0x5B (89..91) (slot 4 keeps 88). Bomb_Start(150,200,0), SE 0xD at f0.
 type 0x24 init 0x411720: re-spawns its own fixed slot as type 0x23 (ring, init 0x4272E0: angle=param.x, radius=param.y, width=param.z), then update := 0x4114E0, segments 44, width 4.
 upd 0x4114E0 (square boundary): t<40: k=1-t/40; width = 88-2t; radius = 192-384k^2; segments -= 1 per frame (44 -> 4 => square). t==40: flash(3,8,1,8FF08080);
  s = radius*0.70711; a = angle+pi/4; for 4 sides: c = centre + polar(a, s); damage rect at c w=8s h=4*width dmg60 life70 period4 rot = a+pi/2; cancel rect w=4s h=4*width item6 life100 same rot; a = a+pi/2.
 CORRECTION: Yukari bomb draw = 0x40D950 (tint 80404040 only). 0x4113A0 is Yukari LS draw (tint 802020D0 + obj0 VMs: VM a (obj0+0x1B8) at obj0.pos+obj0+0x440, z 0.01; VM b (obj0+0x45C) at obj0.pos + its offset, z 0; Draw2DRot).
YUKARI LS 0x410FE0 (draw 0x4113A0): Bomb_Start(250,300,LS); SE 0xD; obj0.pos = player; obj0 VMs scripts 0x15, 0x16 (player anm), RunRange 2.
 T==0/10/20/30: cancel circle player r100 dr1 item6 life 100/40/100/100; damage circle r100 dr1 dmg70 life40 period5; effect 0x25 via SpawnFixedParam at obj0.pos (bomb START position), slots 4..7, same angles, scripts slot5..7 0x5D/0x5E/0x5F.
 type 0x25 init 0x411A80: respawn slot as type 0x23 ring, update 0x4117B0, segments 54, width 6.
 upd 0x4117B0: angle += (slot even -pi/80, odd +pi/80) every frame; t<50: k=1-t/50; width=88-80t/50; radius = (slot-4)*32+192 - ((slot-4)*32+384)*k^2; segments -= 1/frame;
  t==50: shake(1,16,8,0); flash(3,8,1,8F6060F0); 4 side rects as bomb: damage dmg60 life100 period2, cancel life150.
YUYUKO bomb 0x413140 (draw 0x413890 tint 80404040; objects 0..95 drawn, rot z = atan2(frame displacement)):
 f0: Bomb_Start(300,350,0); clear 128 objects; 64 butterflies = 4 groups of 16 (i=0..15): angle = i*pi/8 - pi, centre = player pos at f0 (fixed), r=0;
  groups: (script 0x12, dtheta +pi/240, dr 2), (0x13, -pi/240, 2), (0x12, +pi/200, 1.5), (0x13, -pi/200, 1.5);
  each: damage circle r24 dmg50 life500 max800; cancel circle r24 life500 item6. speed mult 0.8; eff 0x14 at player (not followed); SE 5; shake(1,120,12,0).
 per frame: r += dr; angle += dtheta; pos = centre + polar(angle, r); disp = pos - prev; r<500: areas follow, VM run, VM end -> free+areas off; r>=500 -> free+areas off.
YUYUKO LS 0x413990 (draw 0x4142C0 tint 80802020, all 128 objects): Bomb_Start(300,350,LS); same 64 butterflies; speed mult 0.8; eff 0x14; SE 5; shake(1,60,16,0).
 60<=T<200, adv & T%20==0: base = Rng_SignedRange(pi) [RNG]; take up to 16 free objects (index order): script 0x14; angle = wrap(idx*pi/8 + base) (idx = OBJECT SLOT index QUIRK);
  dtheta field NOT set (stale value of that slot: 0 for never-used slots 64..127; freed butterflies keep theirs) QUIRK; centre = current player; r=0, dr=8;
  damage circle r64 dmg100 life500 max1200; cancel circle r64 life500 item6. Then SE 0xF (pan at last examined object x), shake(1,30,8,0), flash(3,8,1,E0F0F0F0).
 Bomb_Start only sets object state=0 (fields persist between bombs).
OPTIONS (ECX=player, EDX=option; draw 0x44E9E0 for all: VM at option pos + shake, z 0.49, Draw2DRot):
 YUKARI 0x44E3A0: st1: script 0x12; pos = (px, max(py-96,32)); st2; aimed enemy (+0xE2ABC) := 0.
  st2 sub0/1/2: follow 0x44E770: target (px, max(py-96,32)); d = (target-pos)/16; v += (d-v)*0.2; pos += v; |v.x|<0.05 -> v.x=0 (v at option+0x2BC);
    then if shot timer>=0 and aimed enemy and option timer>=10 -> interrupt 3, sub 3; else aimed := 0.
    anim: sub0: v.x>0 -> int2 sub2 scale.x negative; v.x<0 -> int2 sub1 scale.x positive. sub1/2: v.x==0 -> int1 sub0 scale.x positive; opposite sign -> switch (int2).
  sub3 (attack): if aimed: chase 0x44E8D0: target = enemy pos(+0x2D88) + (0,32) (y>=32), same smoothing. if (shot timer<0 && !(input&1)) || !aimed: aimed:=0, int1, sub0.
  st3: t==0 int5; t>=16 st0, fns cleared (same for all options).
 ALICE 0x44EA40: st1 script 0x1D, st2; st2: if not bombing (+0xFDC==0) pos = player-(0,32) (else bomb code moves it); st3: pos = player-(0,32), leave as common.
 REMILIA SOLO 0x44EE70 (idx +0x2D0): st1 script 0x18; anchor(+0x2B0) = player + off: idx0 (-30,-16) angle 0; idx1 (-10,-32) angle pi; idx2 (+10,-32) angle 0; idx3 (+30,-16) angle pi.
  st2: option timer>=12: angle += idx0 +pi/120, idx1 -pi/90, idx2 +pi/90, idx3 -pi/120. colour FFFF8080; if player+3 (focus)==0: colour FF80FFFF and anchor += (player+off - anchor)*0.2 (focused: anchor frozen).
  pos = anchor + polar(angle, 8), z 0; every frame eff 0x2F colour 0x80602050 at option.
 YOUMU HALF (team, option 2) 0x44F2D0 / YOUMU SOLO 0x44F5E0: st1 script 0x15; anchor = player trail[15] (player+0x380, oldest of 16 positions); orbit angle(+0x2D8)=0; facing(+0x2DC)=-pi/2.
  st2: orbit += pi/60; anchor += (trail[15]-anchor)*0.05; pos = anchor + polar(orbit, 8), z 0.
  facing target from direction code (+0xE2A98): 1 up -> +pi/2, 2 down -> -pi/2, 3 left -> 0, 4 right -> pi, 5 UL pi/4, 6 UR 3pi/4, 7 DL -pi/4, 8 DR -3pi/4 (points opposite to the motion); 0 -> unchanged.
  if |facing-target|>pi: target -= sign*2pi; if diff<=pi/2: facing = wrap(facing + (target-facing)*0.07) else facing = target.
  team: eff 0x2F 0x80405080 every frame; solo: unfocused (player+3==0) colour white, facing tracks, eff 0x80405080; focused colour FFFF8080, facing frozen, eff 0xFFF05080.
  shot init 8 aims along option-2 facing (player+0xCD0 = option2+0x2DC).
PLAYER LASER: only Alice's ply01as uses it (6 entries: init 4 laser, update 5 0x450840, draw trail 0x450AD0, hit 2; type 5; origin option 0; interval 2/4/6/8/12/16 = trail length n; scripts 14..18 (+10); dmg 7; box 24x18; speed 10 up).
 laser slots: 3 x {Timer, bullet*} at player+0xE2A38 (array ctor count 3) + entry ptr[] at +0xE2A80. (09 said 4 — fix)
 init 0x44FE20: not bombing, not gm bit13; slot = entry phase (0). empty: slot timer 999, owner=bullet, +0x466 slot, +0x468 origin, +0x444 offset, +0x46A = interval; default spawn; trail[32].x = -999; pos.x = -999; entry ptr; take slot (ret 1).
   slot owned by a different entry (level change): old bullet VM interrupt 1, slot cleared; ret 0. same entry: ret 0.
 update 5 per frame: if not owner, or dialogue, or shot timer<0, or player state 2, or bombing, or gm bit13: VM interrupt 1, slot cleared, update fn := none (bullet then flies on with vel, no cull, freed when its VM ends).
   if option0 state == 0 -> slot cleared, free. else: for i<n: if trail[2i].x >= -900: one-frame damage rect at trail[2i] 16x448 dmg1 nohitfx.
   shift trail (i=31..1: trail[i]=trail[i-1], trail[i].y -= 1); trail[0] = pos; pos = option0 pos; z 0.44; hitbox h = 448; pos.y -= 208; colour youkai-max ? (R FF G D0 B B0) : white. ret 0 -> generic: pos += vel*gs (y -10), VM, timer.
   damage of the bullet itself: Player_DamageAgainst types 4/5 only on even bullet-timer frames (09 §4.5).
 trail draw 0x450AD0: a0 = alpha*3/4; for c=0,2,4.. <2n while trail[c].x != -999: draw VM (Draw2DRot) at trail[c]+shake, alpha = (c==0 ? original : a0 - (a0/2)*c/n); youkai max -> colour (FF,40,40); alpha restored.
 update 4 0x4505D0 unused by data (type-4 laser: slot timer life, capped 20 while dialogue/bomb; scale.y = pos.y/14, hitbox h = pos.y, pos.y /= 2; interrupt 1 when timer<=70 and VM flag bit 14).
