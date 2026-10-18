#!/usr/bin/env python3
# gen.py <variant> <outdir>: build probe_input.txt (frames:mask) + probe_cmd.txt (shots, pokes, quit)
import sys, os
Z, X, UP, DOWN, LEFT, RIGHT = 0x1, 0x2, 0x10, 0x20, 0x40, 0x80
var, out = sys.argv[1], sys.argv[2]
t = 300
presses, shots, cmds = [], [], []
def press(m, wait, hold=3):
    global t
    presses.append((t, m, hold)); t += wait
def shot(name, after=0):
    shots.append((t + after - 5, name))

if var in ('base', 'clr'):
    shot('m_title')
    press(Z, 60); shot('m_diff_enter')
    for i in range(4):
        press(DOWN, 30); shot('m_diff_%d' % i)
    press(Z, 70); shot('m_char_team0')
    for i in range(1, 5):
        press(RIGHT, 70); shot('m_char_team%d' % (i % 4) + ('' if i < 4 else '_again'))
    press(X, 50); shot('m_char_cancel')
    press(X, 60); shot('m_top_back')
if var == 'base':
    press(DOWN, 40); shot('m_top_practice')
    press(Z, 60); shot('m_prac_diff')
    press(Z, 70); shot('m_prac_char')
    press(Z, 70); shot('m_prac_stage')
    press(X, 50); press(X, 50); press(X, 60); shot('m_top_back2')
    press(DOWN, 40); shot('m_top_replay')
    press(Z, 70); shot('m_replay')
    press(X, 60); shot('m_top_back3')
    press(DOWN, 40); shot('m_top_result')
    press(Z, 120); shot('m_result')
    press(X, 60); shot('m_result_x1')
    press(X, 120); shot('m_after_result')
    press(DOWN, 40); shot('m_top_music')
    press(Z, 120); shot('m_music')
    press(X, 120); shot('m_after_music')
    press(DOWN, 40); shot('m_top_option')
    press(Z, 90); shot('m_option')
    for i in range(3):
        press(DOWN, 30); shot('m_option_%d' % i)
    press(X, 40); shot('m_option_quit')
    press(Z, 90); shot('m_after_option')
cmds.append('p1 dl 1')
if var == 'clr':
    for p in (100, 280):
        for team, any_, nc in ((0, 0x41, 0x01), (1, 0x81, 0x81), (2, 0xC1, 0xC1)):
            base = 0x164B998 + team * 0x24
            for d in range(4):
                cmds.append('m%d poke2 %X %X' % (p, base + 0x16 + 2 * d, any_))
                cmds.append('m%d poke2 %X %X' % (p, base + 0x0C + 2 * d, nc))
for f, n in shots:
    cmds.append('m%d shot %s_%s' % (f, var, n))
cmds.append('m%d quit' % (t + 30))
# input file: frame k (1-based) = input word of present k
masks = [0] * (t + 30)
for f, m, h in presses:
    for k in range(f, f + h):
        masks[k - 1] |= m
runs, prev, n = [], masks[0], 0
for m in masks:
    if m == prev: n += 1
    else: runs.append('%d:0x%x' % (n, prev)); prev, n = m, 1
runs.append('%d:0x%x' % (n, prev))
os.makedirs(out, exist_ok=True)
open(os.path.join(out, 'menu_%s.input.txt' % var), 'w').write(','.join(runs) + '\n')
open(os.path.join(out, 'menu_%s.probe_cmd.txt' % var), 'w').write('\n'.join(cmds) + '\n')
open(os.path.join(out, 'menu_%s.shots.txt' % var), 'w').write(''.join('%d %s_%s\n' % (f, var, n) for f, n in shots))
print(var, 'frames', t + 30, 'presses', len(presses), 'shots', len(shots))
