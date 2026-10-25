/* ---- session recorder / player (SESSION_RE_BRIEF). Env: TH08_SESS=rec|play, TH08_SESS_OUT=<win dir>,
   TH08_SESS_IN=<win dir of a recorded session> (play), TH08_SESS_DL=0|1, TH08_SESS_CHUNK=<presents per draw chunk> ---- */
static int g_sess;                    /* 0 off, 1 record, 2 play */
static char g_sess_out[MAX_PATH], g_sess_in[MAX_PATH];
static FILE *g_spoll, *g_sev, *g_strace;
static uint32_t g_poll_n, g_sess_chunk = 1800, g_chunk_frames, g_chunk_seq;
static uint32_t g_rows_since_present, g_res_obj, g_res_hit;
static uint8_t *g_tramp_poll, *g_tramp_joyraw;
static void sess_flush(void);

typedef struct { uint32_t present; uint16_t mask; uint8_t scene, src; } RecPoll;
static RecPoll *g_rp; static uint32_t g_nrp, g_rp_anchor = 0xFFFFFFFF;
static uint32_t g_play_anchor = 0xFFFFFFFF, g_play_mismatch;
static uint32_t g_rec_seed_full; static int g_have_seed;
typedef struct { uint32_t poll; uint8_t b[32]; } RecJoy;
static RecJoy *g_rj; static uint32_t g_nrj, g_joy_n;

/* loader-thread sync: kind 0 = stage Loading_Wait, 1 = stage release (GM+0x38 = 0), 2 = menu release (fade-in,
   +0x14434 = 0), 3 = stage gauge VM setup (Ascii_InitVms / Ascii_SetupGaugeVms), 4 = boot loader (Supervisor_Init) */
static const char *g_lk_name[5] = {"lwait", "stage", "menu", "gauge", "boot"};
static volatile LONG g_lk_parked[5], g_lk_go[5], g_lk_passed[5]; static volatile DWORD g_lk_tid[5];
static uint32_t g_boot_calc, g_boot_done;
typedef struct { uint32_t poll; int kind; } RecSync;
static RecSync g_rs_ev[4096]; static uint32_t g_nrs_ev, g_rs_next;
static int g_blk[3]; static uint32_t g_mm_hit;

static void lk_park(int kind) {
    if (g_sev) { fprintf(g_sev, "loader %s poll %u present %u\n", g_lk_name[kind], g_poll_n, g_present_n + 1); fflush(g_sev); }
    if (g_sess != 2) return;
    g_lk_tid[kind] = GetCurrentThreadId();
    InterlockedExchange(&g_lk_parked[kind], 1);
    for (int t = 0; t < 120000 && !g_lk_go[kind]; t++) Sleep(1);
    InterlockedExchange(&g_lk_go[kind], 0);
    InterlockedExchange(&g_lk_parked[kind], 0);
}
static void __cdecl cb_lwait(uint32_t *sp) { (void)sp; if (GetCurrentThreadId() != g_main_tid) lk_park(0); }
static void __cdecl cb_stage_rel(uint32_t *sp) { (void)sp; lk_park(1); }
static void __cdecl cb_menu_rel(uint32_t *sp) { (void)sp; lk_park(2); }
static void __cdecl cb_gauge(uint32_t *sp) { (void)sp; lk_park(3); }
static void __cdecl cb_boot(uint32_t *sp) { (void)sp; lk_park(4); }
static void __cdecl cb_gauge_done(uint32_t *sp) { (void)sp; InterlockedExchange(&g_lk_passed[3], 1); }
static void __cdecl cb_menu_done(uint32_t *sp) { (void)sp; InterlockedExchange(&g_lk_passed[2], 1); }
static void __cdecl cb_lwait_done(uint32_t *sp) { (void)sp; InterlockedExchange(&g_lk_passed[0], 1); }

/* main thread, play mode: let the parked loader run at the recorded point. A loader that is not there yet may need
   main-thread frames (texture post-load), so after 250 ms the frame runs and the release is retried next frame. */
typedef int(__attribute__((thiscall)) *PFN_Postload)(void *);
static int g_late_frame;
static int lk_release(int kind) {
    int t;
    for (t = 0; t < 3000 && !g_lk_parked[kind]; t++) {
        if (ANMMGR) ((PFN_Postload)0x466000)((void *)ANMMGR);   /* texture post-load the loader may be waiting for */
        Sleep(1);
    }
    if (!g_lk_parked[kind]) { lg("# play: loader %s late at present %u\n", g_lk_name[kind], g_present_n + 1); g_late_frame = 1; return 0; }
    HANDLE th = kind == 1 || kind == 4 ? OpenThread(SYNCHRONIZE, FALSE, g_lk_tid[kind]) : NULL;
    DWORD t0 = GetTickCount();
    InterlockedExchange(&g_lk_passed[kind], 0);
    InterlockedExchange(&g_lk_go[kind], 1);
    if (th) { if (WaitForSingleObject(th, 5000) != WAIT_OBJECT_0) lg("# play: loader %s did not exit\n", g_lk_name[kind]); CloseHandle(th); }
    else { for (t = 0; t < 5000 && !g_lk_passed[kind]; t++) Sleep(1); if (!g_lk_passed[kind]) lg("# play: loader %s did not pass\n", g_lk_name[kind]); }
    lg("# play: loader %s released at present %u, waited %lu ms\n", g_lk_name[kind], g_present_n + 1, GetTickCount() - t0);
    return 1;
}
static void lk_sync(uint32_t k) {
    while (g_rs_next < g_nrs_ev && g_rs_ev[g_rs_next].poll <= k) {
        if (!lk_release(g_rs_ev[g_rs_next].kind)) break;
        g_rs_next++;
    }
}

/* calc entries: the first calc that sees the loader's release (recorded as the sync poll) */
static void blk_seen(int kind, uint32_t v) {
    if (v) { g_blk[kind] = 1; return; }
    if (!g_blk[kind]) return;
    g_blk[kind] = 0;
    if (g_sev) { fprintf(g_sev, "release %s poll %u present %u\n", g_lk_name[kind], g_poll_n - 1, g_present_n + 1); fflush(g_sev); }
}
static void __cdecl cb_gm_calc(uint32_t *sp) { blk_seen(1, U32(sp[7] + 0x38)); }
static int g_mm_screen, g_mm_cursor;
static uint32_t g_mm_obj;
static void __cdecl cb_mm_calc(uint32_t *sp) { g_mm_obj = sp[7]; g_mm_screen = (int)U32(sp[7] + 0x14428); g_mm_cursor = (int)U32(sp[7]); g_mm_hit = 1; blk_seen(2, U32(sp[7] + 0x14434)); }

static FILE *sess_open(const char *dir, const char *name, const char *mode) {
    char p[MAX_PATH * 2]; snprintf(p, sizeof p, "%s\\%s", dir, name);
    return fopen(p, mode);
}

static void sess_open_chunk(void) {
    char n[64]; snprintf(n, sizeof n, "draw_%05u.txt", g_chunk_seq++);
    g_draw = sess_open(g_sess_out, n, "w");
    if (g_draw) setvbuf(g_draw, NULL, _IOFBF, 1 << 22);
    g_chunk_frames = 0;
}

/* TH08_AUTOKEY=replay (record mode, run_rpy.sh): Input_Poll returns scripted keys that open Replay (top item 4) ->
   the only listed file -> its first recorded stage -> speed 0, then nothing; exits 30 polls after the replay leaves
   scene 2. Menu fields 12 §2: +0x00 cursor, +0x0C sub-state, +0x10 sub-state frames, +0x14428 screen; replay screen
   +0xC288 file count (files failing Replay_Load are not listed, 15 §3.8). Keys are 1-poll pulses every 6 polls. */
static int g_autokey, g_ak_started; static uint32_t g_ak_tick, g_ak_end = 0xFFFFFFFF;
static void ak_exit(const char *why) {
    lg("# autokey: exit (%s) at poll %u present %u\n", why, g_poll_n, g_present_n + 1);
    if (g_sev) { fprintf(g_sev, "autokey_exit %s poll %u present %u\n", why, g_poll_n, g_present_n + 1); }
    sess_flush(); ExitProcess(0);
}
static uint16_t autokey(uint8_t scene, uint32_t i) {
    int pulse = (++g_ak_tick % 6) == 0;
    if (g_ak_started) {
        if (scene != 2 && g_ak_end == 0xFFFFFFFF) { g_ak_end = i; lg("# autokey: replay end, scene %u at poll %u present %u\n", scene, i, g_present_n + 1); }
        if (g_ak_end != 0xFFFFFFFF && i >= g_ak_end + 30) ak_exit("end");
        return 0;
    }
    if (scene == 2) { g_ak_started = 1; lg("# autokey: replay start at poll %u present %u\n", i, g_present_n + 1); return 0; }
    if (scene != 1) { g_mm_obj = 0; return 0; }
    uint32_t mm = g_mm_obj;
    if (!mm) return 0;
    uint32_t scr = U32(mm + 0x14428), sub = U32(mm + 0xC), cur = U32(mm);
    if (scr == 0) return pulse ? (cur == 4 ? 0x1 : 0x20) : 0;
    if (scr != 7) return 0;
    if (sub == 1 && U32(mm + 0x10) > 9 && U32(mm + 0xC288) == 0) ak_exit("rejected");
    if (sub == 3 && cur != 0) return pulse ? 0x10 : 0;
    return (sub >= 1 && sub <= 3 && pulse) ? 0x1 : 0;
}

/* frames: present number of the frame being computed (its Present has not happened yet) */
static uint16_t __cdecl w_input_poll(void) {
    uint32_t ret = (uint32_t)__builtin_return_address(0);
    uint8_t src = ret >= 0x471181 && ret < 0x471500 ? 'I' : 'S';
    uint8_t scene = (uint8_t)U32(G_SCENE);
    uint16_t m;
    uint32_t i = g_poll_n++;
    if (i == 0 && g_sev) { fprintf(g_sev, "release boot calc %u present %u\n", g_calc_n + 1, g_present_n + 1); fflush(g_sev); }
    if (g_sess == 2) {
        uint32_t k;
        if (g_play_anchor == 0xFFFFFFFF && scene == 1) g_play_anchor = i;
        if (g_play_anchor == 0xFFFFFFFF) k = i < g_rp_anchor ? i : (g_rp_anchor ? g_rp_anchor - 1 : 0);
        else {
            if (g_late_frame && i > g_play_anchor) g_play_anchor++;   /* extra frame while a loader is late: repeat the previous word */
            k = g_rp_anchor + (i - g_play_anchor);
        }
        m = k < g_nrp ? g_rp[k].mask : 0;
        if (k < g_nrp && (g_rp[k].scene != scene || g_rp[k].src != src) && g_play_mismatch++ < 20)
            lg("# PLAY MISMATCH poll %u (rec %u): scene %u/%u src %c/%c present %u\n", i, k, scene, g_rp[k].scene, src, g_rp[k].src, g_present_n + 1);
        if (k == g_nrp) lg("# play: recorded input exhausted at poll %u present %u\n", i, g_present_n + 1);
        if (k >= g_nrp + 600) { lg("# play: exit, 600 polls past the recording\n"); sess_flush(); ExitProcess(0); }
    } else {
        m = ((uint16_t(*)(void))g_tramp_poll)();
        if (g_autokey) m = autokey(scene, i);
    }
    if (g_spoll) fprintf(g_spoll, "%u %u %u %c %04x\n", i, g_present_n + 1, scene, src, m);
    return m;
}

static uint8_t *__cdecl w_joy_raw(void) {
    uint8_t *b;
    if (g_sess == 2) {
        b = (uint8_t *)0x16535A0; memset(b, 0, 128);
        for (uint32_t j = 0; j < g_nrj; j++) if (g_rj[j].poll == g_joy_n) { memcpy(b, g_rj[j].b, 32); break; }
    } else b = ((uint8_t *(*)(void))g_tramp_joyraw)();
    if (g_sev) {
        int nz = 0; for (int j = 0; j < 32; j++) nz |= b[j];
        if (nz) { fprintf(g_sev, "joyraw %u", g_joy_n); for (int j = 0; j < 32; j++) fprintf(g_sev, " %02x", b[j]); fprintf(g_sev, "\n"); fflush(g_sev); }
    }
    g_joy_n++;
    return b;
}

static void __cdecl cb_seed(uint32_t *sp) {
    if (g_sess == 2 && g_have_seed) { U32(0x17CE900) = g_rec_seed_full; sp[8] = g_rec_seed_full; }
    if (g_sev) { fprintf(g_sev, "seed %08x present %u poll %u\n", sp[8], g_present_n + 1, g_poll_n); fflush(g_sev); }
}
static void __cdecl cb_setstate(uint32_t *sp) {
    if (g_sev) { fprintf(g_sev, "setstate %04x obj %08x present %u poll %u\n", sp[10] & 0xFFFF, sp[7], g_present_n + 1, g_poll_n); fflush(g_sev); }
}
static void __cdecl cb_result_calc(uint32_t *sp) { g_res_obj = sp[7]; g_res_hit = 1; }


/* TH08_SCAN="P0 P1": per calc row with poll in [P0,P1] in scene 2, dump enemies/items/bomb areas to scan.txt (RE-Q18) */
static int g_scan0 = -1, g_scan1 = -1; static FILE *g_scanf;
static void sess_scan(int pass) {
    if (g_scan0 < 0 || (int)g_poll_n < g_scan0 || (int)g_poll_n > g_scan1 || U32(G_SCENE) != 2 || U32(G_LOADING)) return;
    if (!g_scanf) g_scanf = sess_open(g_sess_out, "scan.txt", "w");
    if (!g_scanf) return;
    FILE *o = g_scanf; uint32_t P = G_PLAYER, st = U32(G_STATS), am = U32(0x18B89B4);
    int aslot = am ? (int)((am - 0x577F20 - 0x53D0) / 0x53D0) : -1;
    fprintf(o, "F poll %u pass %d frame %u rng %04x ctr %u player %08x %08x pst %u p3 %u p5 %u fdc %d fe4 %d e2a7c %d shot %d aimed %d tgt %08x %08x flag %d pow %08x bul %u\n",
            g_poll_n, pass, U32(G_GMFRAME), U16(G_RNG), g_rng_calls, U32(0x17D61AC), U32(0x17D61B0), U8(P), U8(P + 3), U8(P + 5),
            (int)U32(P + 0xFDC), (int)U32(P + 0xFE4), (int)U32(P + 0xE2A7C), (int)U32(0x18B89BC + 8), aslot, U32(P + 0xE2AA4), U32(P + 0xE2AA8), (int)U32(P + 0xE2AC0),
            st >= 0x10000 ? U32(st + 0x98) : 0, U32(G_BULLETS));
    for (int k = 0; k < 4; k++) {
        uint32_t op = P + 0x40C + k * 0x2F4;
        if (!U32(op + 0x2EC)) continue;
        fprintf(o, "O %d", k);
        for (int q = 0x2A4; q < 0x2F4; q += 4) fprintf(o, " %08x", U32(op + q));
        fprintf(o, "\n");
    }
    for (int k = 0; k < 128; k++) {
        uint32_t b = P + 0xBE838 + k * 0x484;
        if (!U16(b + 0x462)) continue;
        fprintf(o, "B %d st %d ty %d pos %08x %08x sz %08x %08x vel %08x %08x dmg %d t %d\n", k, (int)(int16_t)U16(b + 0x462), (int)(int16_t)U16(b + 0x464),
                U32(b + 0x2A4), U32(b + 0x2A8), U32(b + 0x430), U32(b + 0x434), U32(b + 0x43C), U32(b + 0x440), (int)(int16_t)U16(b + 0x460), (int)U32(b + 0x454 + 8));
    }
    if (am) {
        fprintf(o, "X %08x fl %08x %08x pos %08x %08x base %08x %08x life %d t %d par %08x sub %d raw", am, U32(am + 0x3324), U32(am + 0x3328), U32(am + 0x2D88), U32(am + 0x2D8C),
                U32(am + 0x2D34), U32(am + 0x2D38), (int)U32(am + 0x2DFC), (int)U32(am + 0x2E14 + 8), U32(am + 0x2DA4), (int)U32(am + 0x2DF8));
        for (int q = 0x2D00; q < 0x2E40; q += 4) fprintf(o, " %08x", U32(am + q));
        fprintf(o, "\n");
    }
    for (int j = 0; j < 480; j++) {
        uint32_t e = 0x577F20 + 0x53D0 + j * 0x53D0;
        if (!(U32(e + 0x3324) & 1)) continue;
        fprintf(o, "E %d fl %08x %08x pos %08x %08x base %08x %08x hb %08x %08x life %d item %d t %d par %08x sub %d\n", j, U32(e + 0x3324), U32(e + 0x3328),
                U32(e + 0x2D88), U32(e + 0x2D8C), U32(e + 0x2D34), U32(e + 0x2D38), U32(e + 0x2D70), U32(e + 0x2D74), (int)U32(e + 0x2DFC), (int)U32(e + 0x3304),
                (int)U32(e + 0x2E14 + 8), U32(e + 0x2DA4), (int)U32(e + 0x2DF8));
        { uint32_t spr = U32(e + 0x230); fprintf(o, "S %d spr %08x %08x\n", j, spr ? U32(spr + 0x34) : 0, spr ? U32(spr + 0x30) : 0); }
    }
    for (int j = 0; j < 384; j++) {
        uint32_t a = P + 0xB8834 + j * 0x40;
        if (!U8(a + 0x3C)) continue;
        fprintf(o, "A %d", j);
        for (int q = 0; q < 0x40; q += 4) fprintf(o, " %08x", U32(a + q));
        fprintf(o, "\n");
    }
    if (getenv("TH08_SCANBUL")) for (int j = 0; j < 1536; j++) {
        uint32_t b = 0xF54E90 + 0x1A880 + j * 0x10B8;
        if (!U16(b + 0xDB8)) continue;
        uint32_t spr = U32(b + 0x224);
        fprintf(o, "Q %d st %d pos %08x %08x vel %08x %08x ang %08x spd %08x off %d grace %d act %x fl %x col %d wh %08x %08x\n",
                j, (int16_t)U16(b + 0xDB8), U32(b + 0xD44), U32(b + 0xD48), U32(b + 0xD50), U32(b + 0xD54), U32(b + 0xD74), U32(b + 0xD68),
                (int16_t)U16(b + 0xDBA), (int)U32(b + 0xDA8), U32(b + 0xDAC), U32(b + 0xDB0), (int16_t)U16(b + 0xDB4), spr ? U32(spr + 0x34) : 0, spr ? U32(spr + 0x30) : 0);
    }
    uint32_t it = U32(0x1653648 + 0x17B088); int n = 0;
    while (it && n < 3000) {
        fprintf(o, "I %u ty %u st %u mx %u pos %08x %08x vel %08x %08x t %d\n", (it - 0x1653648) / 0x2E4, U8(it + 0x2D4), U8(it + 0x2D7), U8(it + 0x2D8),
                U32(it + 0x2A4), U32(it + 0x2A8), U32(it + 0x2B0), U32(it + 0x2B4), (int)U32(it + 0x2C8 + 8));
        it = U32(it + 0x2DC); n++;
    }
    fflush(o);
}

/* TH08_BOSS="P0 P1" (RE-Q19): per calc row with poll in [P0,P1] in scene 2 while any enemy has flag A bit 1 (boss),
   G = globals (spell flags/bonus/timer, bullets, player state/timer, HUD boss flag), B = each boss enemy, T = its threads */
static int g_boss0 = -1, g_boss1 = -1; static int g_srs0 = -1, g_srs1 = -1; static FILE *g_bossf;
static void sess_boss(int pass) {
    if (g_boss0 < 0 || (int)g_poll_n < g_boss0 || (int)g_poll_n > g_boss1 || U32(G_SCENE) != 2 || U32(G_LOADING)) return;
    int any = 0;
    for (int j = 0; j < 480; j++) { uint32_t e = 0x577F20 + 0x53D0 + j * 0x53D0; if ((U32(e + 0x3324) & 3) == 3) { any = 1; break; } }
    if (!any && !U32(G_BOSS) && !(U8(G_SPELL) & 1)) return;
    if (!g_bossf) g_bossf = sess_open(g_sess_out, "boss.txt", "w");
    if (!g_bossf) return;
    FILE *o = g_bossf; uint32_t P = G_PLAYER;
    int en = 0;
    for (int i = 0; i < 480; i++) if (U8(G_ENEMIES + 0x53D0 + i * 0x53D0 + 0x3324) & 1) en++;
    fprintf(o, "G poll %u pass %d frame %u rng %04x spf %08x spbonus %d spid %d sptimer %d %d bul %u en %d pst %u pt %d bomb %d hudboss %u boss0 %08x gf %08x itm %u\n",
            g_poll_n, pass, U32(G_GMFRAME), U16(G_RNG), U32(G_SPELL), (int)U32(G_SPELL + 0xFC), (int)U32(G_SPELL + 8),
            (int)U32(G_SPELL + 0x108 + 8), (int)U32(G_SPELL + 0x114 + 8), U32(G_BULLETS), en, U8(P), (int)U32(0x18B89EC + 8),
            (int)U32(0x17D6ED4), U8(0x160F428 + 0x2C), U32(G_BOSS), U32(G_GMFLAGS), U32(0x1653648 + 0x17B08C));
    { uint32_t st = U32(G_STATS); int orbs = 0; uint32_t it = U32(0x1653648 + 0x17B088); int n = 0;
      while (it && n < 3000) { if (U8(it + 0x2D4) == 7) orbs++; it = U32(it + 0x2DC); n++; }
      { uint32_t cur = U32(0x1653648 + 0x17ADA4); fprintf(o, "C itemcursor %u busy %d\n", cur, cur < 0x830 ? (int)U8(0x1653648 + cur * 0x2E4 + 0x2D5) : -1); }
      fprintf(o, "L time %d thr %d pend %d orbs %d diff %u stage %u\n", st >= 0x10000 ? (int)U32(st + 0x3C) : -1, st >= 0x10000 ? (int)U32(st + 0x40) : -1,
              (int)U32(G_SPELL + 0x10), orbs, U32(0x160F538), U32(G_STAGE)); }
    uint32_t subs = U32(0x4ECCB8 + 4);
    for (int j = 0; j < 480; j++) {
        uint32_t e = 0x577F20 + 0x53D0 + j * 0x53D0;
        if ((U32(e + 0x3324) & 3) != 3) continue;
        uint32_t c = e + 0x7F8; int sub = (int16_t)U16(c + 0x224);
        uint32_t ipo = (sub >= 0 && subs) ? U32(c) - U32(subs + sub * 4) : U32(c);
        fprintf(o, "B %d sub %d ip %x clk %d wait %d dep %d life %d max %d hud %d thr %d %d %d %d tsub %d %d %d %d tmo %d tmosub %d dsub %d tim %d dmgt %d fl %08x %08x pos %08x %08x bon %d slot %u int %d\n",
                j, sub, ipo, (int)U32(c + 4 + 8), (int)U32(c + 0x90 + 8), (int16_t)U16(e + 0x2CEA), (int)U32(e + 0x2DFC), (int)U32(e + 0x2E00), (int)U32(e + 0x2E04),
                (int)U32(e + 0x3358), (int)U32(e + 0x335C), (int)U32(e + 0x3360), (int)U32(e + 0x3364),
                (int)U32(e + 0x3368), (int)U32(e + 0x336C), (int)U32(e + 0x3370), (int)U32(e + 0x3374),
                (int)U32(e + 0x3378), (int)U32(e + 0x337C), (int16_t)U16(e + 0x2CEE), (int)U32(e + 0x2E14 + 8), (int)U32(e + 0x5354 + 8),
                U32(e + 0x3324), U32(e + 0x3328), U32(e + 0x2D88), U32(e + 0x2D8C), (int)U32(e + 0x53CC), U8(e + 0x3313), (int16_t)U16(e + 0x2D30));
        fprintf(o, "V %d f", j); for (int q = 0x38; q < 0x58; q += 4) fprintf(o, " %08x", U32(c + q)); fprintf(o, "\n");
        for (int k = 0; k < 4; k++) {
            uint32_t t = U32(e + 0x3384 + 4 * k);
            if (!t) continue;
            uint32_t tc = t + 8; int ts = (int16_t)U16(tc + 0x224);
            fprintf(o, "T %d %d sub %d ip %x clk %d wait %d\n", j, k, ts, (ts >= 0 && subs) ? U32(tc) - U32(subs + ts * 4) : U32(tc),
                    (int)U32(tc + 4 + 8), (int)U32(tc + 0x90 + 8));
        }
    }
    fflush(o);
}

/* one trace row per calc pass; columns = demo trace columns, then the session extension */
/* TH08_ACTR=1: ASCII manager frame counter +0x8284 per trace row -> actr.txt (RE-Q27) */
static int g_actr = -1; static FILE *g_actrf;
static void sess_actr(void) {
    if (g_actr < 0) { const char *e = getenv("TH08_ACTR"); g_actr = e && atoi(e); }
    if (!g_actr) return;
    if (!g_actrf) g_actrf = sess_open(g_sess_out, "actr.txt", "w");
    if (g_actrf) fprintf(g_actrf, "%u %u %u\n", g_poll_n, U32(0x4CCE20 + 0x8284), U32(G_SCENE));
}
static void sess_row(int pass) {
    if (!g_strace) return;
    sess_actr();
    uint32_t st = U32(G_STATS), sc = U32(G_SCENE);
    uint32_t s8 = 0, sC = 0, s30 = 0, s74 = 0, s80 = 0, s98 = 0; int s22 = 0, s3C = 0;
    if (sc == 2 && !U32(G_LOADING) && st >= 0x10000) { s8 = U32(st + 8); sC = U32(st + 0xC); s30 = U32(st + 0x30); s74 = U32(st + 0x74); s80 = U32(st + 0x80); s98 = U32(st + 0x98); s22 = (int16_t)U16(st + 0x22); s3C = (int)U32(st + 0x3C); }
    int en = 0;
    for (int i = 0; i < 480; i++) if (U8(G_ENEMIES + 0x53D0 + i * 0x53D0 + 0x3324) & 1) en++;
    int boss = 0;
    for (int i = 0; i < 4; i++) if (U32(G_BOSS + 4 * i)) boss |= 1 << i;
    int menu = -1, cur = -1, res = -1;
    if (g_mm_hit) { menu = g_mm_screen; cur = g_mm_cursor; }
    g_mm_hit = 0;
    if (g_res_hit && g_res_obj >= 0x10000) res = (int)U32(g_res_obj + 8);
    g_res_hit = 0;
    fprintf(g_strace, "%u %u %04x %04x %u %08x %08x %u %u %u %u %08x %08x %08x %d %d %u %d %d %u %d %x %d"
                      " %u %u %u %u %u %d %d %d %u %04x %u %u %d\n",
            U32(G_GMFRAME), in_demo() ? U32(G_DEMOFR) : 0, U16(G_INPUT), U16(G_RNG), g_rng_calls,
            U32(G_PLAYER + 0x2B4), U32(G_PLAYER + 0x2B8), U8(G_PLAYER),
            s8, sC, s30, s74, s80, s98, s22, s3C, U32(G_BULLETS), en, (int)U32(G_RANK), U8(G_SPELL) & 1, boss, U32(G_GMFLAGS), pass,
            g_present_n + 1, g_calc_n, g_poll_n, sc, sc != g_scene_prev ? 1 : g_scene_n + 1, menu, cur, res, U8(G_PAUSE), U16(0x164D528), U32(G_STAGE), U32(G_LOADING),
            in_demo() ? (int)U8(G_DEMOIDX) : -1);
    g_rows_since_present++;
    if (pass >= 0) { sess_scan(pass); sess_boss(pass); }
    if (g_srs0 >= 0) {
        if (g_rs_on && g_rsf) { fprintf(g_rsf, "R poll %u frame %u ctr %u rng %04x bullets %u\n", g_poll_n, U32(G_GMFRAME), g_rng_calls, U16(G_RNG), U32(G_BULLETS)); fflush(g_rsf); }
        g_rs_on = (int)g_poll_n + 1 >= g_srs0 && (int)g_poll_n + 1 <= g_srs1;
    }
}

static int g_spass;
static void __cdecl cb_sess_passstart(void) {
    if (g_spass > 0) sess_row(g_spass);
    g_spass++;
}
/* Frame_Run, just before FuncChain_RunCalc: loader syncs (the loader's own calls must not overlap a running chain) */
static void __cdecl cb_sess_framestart(void) {
    g_late_frame = 0;
    if (g_sess == 2 && g_boot_calc && !g_boot_done && g_calc_n + 1 >= g_boot_calc) g_boot_done = lk_release(4);
    if (g_sess == 2 && g_play_anchor != 0xFFFFFFFF) lk_sync(g_rp_anchor + (g_poll_n - g_play_anchor));
}
/* noclip (RE_NOCLIP_BRIEF): Player_Die 0x44AB40 (thiscall, no stack args) becomes "count; ret". Its callers have already
   done the hit-box test, the replay-stats flag (+0xDA |= 2), the state == 0 check and GameMgr_ScrambleDecoys. */
volatile uint32_t g_noclip_hits;
static uint32_t g_noclip_on, g_noclip_logged;
__asm__(".globl _noclip_die\n_noclip_die:\n\tlock incl _g_noclip_hits\n\tret\n");
extern void noclip_die(void);
/* TH08_FAST: virtual Timer_Now (thiscall, no args, result in ST0). +25 ms per call (2 calls per frame, so every
   Frame_Run is past its deadline), but real elapsed time while a loader thread runs (Supervisor+0x284 0x17CE9DC != 0):
   the loaders sleep in 17 ms steps and wait for main-thread frames, so loading keeps the normal 60 Hz pace. */
static int64_t g_fast_us, g_fast_qpc, g_fast_freq;
static double __cdecl fast_now(void) {
    LARGE_INTEGER q; QueryPerformanceCounter(&q);
    if (U32(0x17CE9DC)) g_fast_us += (q.QuadPart - g_fast_qpc) * 1000000 / g_fast_freq;
    else g_fast_us += 25000;
    g_fast_qpc = q.QuadPart;
    return (double)g_fast_us * 1e-6;
}
static void __cdecl cb_sess_postcalc(void) {
    uint16_t cw = fpu_cw();
    g_calc_n++;
    if (g_noclip_on && g_noclip_hits != g_noclip_logged && g_sev) {
        g_noclip_logged = g_noclip_hits;
        fprintf(g_sev, "noclip_hit %u calc %u present %u\n", g_noclip_logged, g_calc_n, g_present_n + 1); fflush(g_sev);
    }
    g_rng_at_calc = g_rng_main;
    if (cw != g_last_cw_calc) { lg("# Frame_Run post-calc cw=%04x at calc %u present %u\n", cw, g_calc_n, g_present_n); g_last_cw_calc = cw; }
    g_spass--;
    sess_row(g_spass < 0 ? 0 : g_spass);
    g_spass = 0;
}

static void sess_present(void) {
    if ((g_present_n % 300) == 0) lg("# time present %u scene %u ms %lu\n", g_present_n, U32(G_SCENE), GetTickCount());
    if (!g_rows_since_present) sess_row(-1);   /* presents without a Frame_Run calc (Info_Show loop) */
    g_rows_since_present = 0;
    if (g_strace && (g_present_n & 63) == 0) fflush(g_strace);
    if (g_spoll && (g_present_n & 63) == 0) fflush(g_spoll);
    if (g_draw && g_dl_mode && ++g_chunk_frames >= g_sess_chunk) {
        fclose(g_draw);
        sess_open_chunk();
    }
}

static void sess_flush(void) {
    if (g_strace) fflush(g_strace);
    if (g_spoll) fflush(g_spoll);
    if (g_sev) fflush(g_sev);
    if (g_draw) fflush(g_draw);
}

static uint8_t *make_tramp(uint32_t addr, int len) {
    uint8_t *t = VirtualAlloc(NULL, 32, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
    memcpy(t, (void *)addr, len);
    t[len] = 0xE9; *(int32_t *)(t + len + 1) = (int32_t)((addr + len) - (uint32_t)(t + len + 5));
    return t;
}
static void jmp_patch(uint32_t addr, void *to, int len) {
    DWORD old; uint8_t *p = (uint8_t *)addr;
    VirtualProtect(p, len, PAGE_EXECUTE_READWRITE, &old);
    p[0] = 0xE9; *(int32_t *)(p + 1) = (int32_t)((uint32_t)to - (addr + 5));
    for (int i = 5; i < len; i++) p[i] = 0x90;
    VirtualProtect(p, len, old, &old);
    FlushInstructionCache(GetCurrentProcess(), p, len);
}

static void sess_load_rec(void) {
    FILE *f = sess_open(g_sess_in, "polls.txt", "r"); char line[256];
    if (!f) { lg("# play: no polls.txt in %s\n", g_sess_in); return; }
    uint32_t cap = 1 << 16; g_rp = malloc(cap * sizeof *g_rp);
    while (fgets(line, sizeof line, f)) {
        unsigned i, pr, sc, m; char src;
        if (line[0] == '#' || sscanf(line, "%u %u %u %c %x", &i, &pr, &sc, &src, &m) != 5) continue;
        if (g_nrp == cap) { cap *= 2; g_rp = realloc(g_rp, cap * sizeof *g_rp); }
        g_rp[g_nrp].present = pr; g_rp[g_nrp].mask = (uint16_t)m; g_rp[g_nrp].scene = (uint8_t)sc; g_rp[g_nrp].src = (uint8_t)src;
        if (g_rp_anchor == 0xFFFFFFFF && sc == 1) g_rp_anchor = g_nrp;
        g_nrp++;
    }
    fclose(f);
    if (g_rp_anchor == 0xFFFFFFFF) g_rp_anchor = g_nrp;
    f = sess_open(g_sess_in, "events.txt", "r");
    if (f) {
        uint32_t jcap = 64; g_rj = malloc(jcap * sizeof *g_rj);
        while (fgets(line, sizeof line, f)) {
            unsigned v;
            char kn[16];
            if (!g_have_seed && sscanf(line, "seed %x", &v) == 1) { g_rec_seed_full = v; g_have_seed = 1; }
            else if (g_nrs_ev < 4096 && sscanf(line, "loader lwait poll %u", &v) == 1) { g_rs_ev[g_nrs_ev].poll = v; g_rs_ev[g_nrs_ev++].kind = 0; }
            else if (sscanf(line, "release boot calc %u", &v) == 1) g_boot_calc = v;
            else if (g_nrs_ev < 4096 && sscanf(line, "loader gauge poll %u", &v) == 1) { g_rs_ev[g_nrs_ev].poll = v; g_rs_ev[g_nrs_ev++].kind = 3; }
            else if (g_nrs_ev < 4096 && sscanf(line, "release %15s poll %u", kn, &v) == 2) { g_rs_ev[g_nrs_ev].poll = v; g_rs_ev[g_nrs_ev++].kind = !strcmp(kn, "stage") ? 1 : 2; }
            else if (!strncmp(line, "joyraw ", 7)) {
                if (g_nrj == jcap) { jcap *= 2; g_rj = realloc(g_rj, jcap * sizeof *g_rj); }
                char *p = line + 7; g_rj[g_nrj].poll = strtoul(p, &p, 10);
                for (int j = 0; j < 32; j++) g_rj[g_nrj].b[j] = (uint8_t)strtoul(p, &p, 16);
                g_nrj++;
            }
        }
        fclose(f);
    }
    for (uint32_t a = 1; a < g_nrs_ev; a++) for (uint32_t b = a; b > 0 && g_rs_ev[b - 1].poll > g_rs_ev[b].poll; b--) { RecSync t = g_rs_ev[b]; g_rs_ev[b] = g_rs_ev[b - 1]; g_rs_ev[b - 1] = t; }
    lg("# play: %u polls, anchor %u, seed %s %08x, %u joyraw, %u loader syncs\n", g_nrp, g_rp_anchor, g_have_seed ? "yes" : "no", g_rec_seed_full, g_nrj, g_nrs_ev);
}

/* called from DllMain; returns 1 when session mode is on */
static int sess_init(void) {
    const char *m = getenv("TH08_SESS"), *o = getenv("TH08_SESS_OUT"), *in = getenv("TH08_SESS_IN"), *dl = getenv("TH08_SESS_DL"), *ch = getenv("TH08_SESS_CHUNK");
    if (!m || !o) return 0;
    g_sess = !strcmp(m, "play") ? 2 : 1;
    snprintf(g_sess_out, sizeof g_sess_out, "%s", o);
    if (in) snprintf(g_sess_in, sizeof g_sess_in, "%s", in);
    if (ch && atoi(ch) > 0) g_sess_chunk = atoi(ch);
    { const char *sc = getenv("TH08_SCAN"); if (sc) sscanf(sc, "%d %d", &g_scan0, &g_scan1); }
    { const char *sc = getenv("TH08_RNGSITE"); if (sc) sscanf(sc, "%d %d", &g_srs0, &g_srs1); }
    { const char *sc = getenv("TH08_BOSS"); if (sc) sscanf(sc, "%d %d", &g_boss0, &g_boss1); }
    { const char *ak = getenv("TH08_AUTOKEY"); g_autokey = g_sess == 1 && ak && !strcmp(ak, "replay"); }
    CreateDirectoryA(g_sess_out, NULL);
    g_log = sess_open(g_sess_out, "probe_log.txt", "w");
    g_spoll = sess_open(g_sess_out, "polls.txt", "w");
    g_sev = sess_open(g_sess_out, "events.txt", "w");
    g_strace = sess_open(g_sess_out, "trace.txt", "w");
    if (g_spoll) { setvbuf(g_spoll, NULL, _IOFBF, 1 << 16); fprintf(g_spoll, "# poll present scene src mask\n"); }
    if (g_strace) {
        setvbuf(g_strace, NULL, _IOFBF, 1 << 18);
        fprintf(g_strace, "# frame demo_frame input rng rng_ctr px py pstate score graze points lives bombs power gauge time bullets enemies rank spell boss flags ff"
                          " present calc poll scene scene_frame menu cursor result pause rawinput stage loading demo\n");
    }
    g_main_tid = GetCurrentThreadId();
    lg("# probe loaded (session %s), real d3d8=%p, DllMain cw=%04x tid %lu\n", g_sess == 2 ? "play" : "record", g_real, fpu_cw(), g_main_tid);
    if (g_sess == 2) sess_load_rec();
    if (dl && atoi(dl)) { g_dl_mode = 1; sess_open_chunk(); }
    g_tramp_poll = make_tramp(0x43D970, 9); jmp_patch(0x43D970, w_input_poll, 9);
    g_tramp_joyraw = make_tramp(0x43D7E0, 9); jmp_patch(0x43D7E0, w_joy_raw, 9);
    inline_hook_sp(0x4464FF, cb_seed, 6);
    inline_hook_sp(0x453BE0, cb_setstate, 7);
    inline_hook_sp(0x4584B0, cb_result_calc, 6);
    inline_hook_sp(0x439BC7, cb_gm_calc, 6);
    inline_hook_sp(0x467399, cb_mm_calc, 6);
    inline_hook_sp(0x448972, cb_lwait, 7);
    inline_hook_sp(0x43B88B, cb_lwait_done, 9);
    inline_hook_sp(0x43B821, cb_gauge, 5);
    inline_hook_sp(0x4469CE, cb_boot, 7);
    inline_hook_sp(0x43B84A, cb_gauge_done, 5);
    inline_hook_sp(0x43B8A8, cb_stage_rel, 7);
    inline_hook_sp(0x4710DF, cb_menu_rel, 7);   /* before Music_Play / title fade-in / release store */
    inline_hook_sp(0x471176, cb_menu_done, 7);  /* after the store and Loading_End */
    inline_hook(0x441F52, cb_sess_postcalc, 8);
    inline_hook(0x441F48, cb_sess_framestart, 5);
    inline_hook(0x43CA65, cb_sess_passstart, 7);
    inline_hook_sp(0x43ECC0, cb_rng, 6);
    inline_hook(0x43ABD7, cb_stagestart, 6);
    inline_hook_sp(0x465860, cb_anm_preload, 6);
    inline_hook_sp(0x4667C0, cb_surf_load, 6);
    { const char *e = getenv("TH08_LDLOG"); if (e && atoi(e)) inline_hook_sp(0x465BA0, cb_postload, 6); }
    { const char *fa = getenv("TH08_FAST");
      if (fa && atoi(fa)) {
          /* fast mode: Timer_Now 0x442190 (only caller Frame_Run) returns the virtual clock fast_now */
          LARGE_INTEGER q; QueryPerformanceFrequency(&q); g_fast_freq = q.QuadPart; QueryPerformanceCounter(&q); g_fast_qpc = q.QuadPart;
          jmp_patch(0x442190, (void *)fast_now, 5);
          lg("# fast: Timer_Now 0x442190 -> virtual clock\n"); } }
    { const char *nc = getenv("TH08_NOCLIP");
      if (nc && atoi(nc)) {
          /* skipping Player_Die skips its GameMgr_UpdateChecksum after the caller's ScrambleDecoys, so the decoy check
             GameMgr_IsTampered 0x40BB80 would report tampering (quit at EffectMgr frame %300 == 100, bombs/power/lives
             refused). It is a pure check that is 0 in every untampered run: make it always 0 (no RNG, no state). */
          static const uint8_t ret0[3] = { 0x31, 0xC0, 0xC3 };
          DWORD old; VirtualProtect((void *)0x40BB80, 3, PAGE_EXECUTE_READWRITE, &old);
          memcpy((void *)0x40BB80, ret0, 3); VirtualProtect((void *)0x40BB80, 3, old, &old);
          FlushInstructionCache(GetCurrentProcess(), (void *)0x40BB80, 3);
          g_noclip_on = 1; jmp_patch(0x44AB40, (void *)noclip_die, 5);
          lg("# noclip: Player_Die 0x44AB40 -> count+ret, GameMgr_IsTampered 0x40BB80 -> 0\n"); } }
    return 1;
}
