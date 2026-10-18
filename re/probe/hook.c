// th08 runtime probe: proxy d3d8.dll (dirty-room tooling only)
#include <windows.h>
#include <stdio.h>
#include <stdint.h>
#include <string.h>

#define U8(a) (*(volatile uint8_t *)(a))
#define U16(a) (*(volatile uint16_t *)(a))
#define U32(a) (*(volatile uint32_t *)(a))

static FILE *g_log, *g_trace;
static HMODULE g_real;
static DWORD g_main_tid;
static volatile uint32_t g_rng_calls, g_rng_main;
static uint32_t g_present_n, g_calc_n;
static uint16_t g_last_cw_calc = 0xFFFF, g_last_cw_present = 0xFFFF;
static int g_trace_all, g_nodik, g_gks_calls, g_effscan_until;
static uint32_t g_rng_at_calc, g_draw_rng_frames;

typedef void *(WINAPI *PFN_Create8)(UINT);
typedef HRESULT(WINAPI *PFN_CreateDevice)(void *, UINT, int, HWND, DWORD, void *, void **);
typedef HRESULT(WINAPI *PFN_Present)(void *, const RECT *, const RECT *, HWND, const void *);
typedef HRESULT(WINAPI *PFN_Reset)(void *, void *);
static PFN_CreateDevice o_CreateDevice;
static PFN_Present o_Present;
static PFN_Reset o_Reset;
typedef SHORT dummy_t;
typedef BOOL(WINAPI *PFN_GKS)(PBYTE);
static PFN_GKS o_GetKeyboardState;

static uint16_t fpu_cw(void) { uint16_t cw; __asm__ volatile("fnstcw %0" : "=m"(cw)); return cw; }

static void lg(const char *fmt, ...) {
    va_list ap; va_start(ap, fmt);
    vfprintf(g_log, fmt, ap); va_end(ap); fflush(g_log);
}

/* ---- command script ---- */
enum { C_PRESENT, C_DEMO, C_SCENE, C_MENU };
static uint32_t g_in_base;
static uint32_t g_scene_prev = 0xFFFFFFFF, g_scene_n;
typedef struct { int ctype, k, f; char act[16]; uint32_t a, b, c; char name[64]; int done; } Cmd;
static Cmd g_cmds[256]; static int g_ncmd;
static BYTE g_keys[256]; static int g_key_until[256];

static void load_cmds(void) {
    FILE *f = fopen("probe_cmd.txt", "r"); char line[256];
    if (!f) return;
    while (fgets(line, sizeof line, f) && g_ncmd < 256) {
        Cmd *c = &g_cmds[g_ncmd]; memset(c, 0, sizeof *c);
        char cond[32];
        if (!strncmp(line, "traceall", 8)) { g_trace_all = 1; continue; }
        if (line[0] == '#' || sscanf(line, "%31s %15s", cond, c->act) < 2) continue;
        if (!strcmp(cond, "traceall")) { g_trace_all = 1; continue; }
        if (cond[0] == 'p') { c->ctype = C_PRESENT; c->f = atoi(cond + 1); }
        else if (cond[0] == 'd') { c->ctype = C_DEMO; sscanf(cond + 1, "%d:%d", &c->k, &c->f); }
        else if (cond[0] == 'm') { c->ctype = C_MENU; c->f = atoi(cond + 1); }
        else if (cond[0] == 's') { c->ctype = C_SCENE; sscanf(cond + 1, "%d:%d", &c->k, &c->f); }
        else continue;
        char *rest = strstr(line, c->act) + strlen(c->act);
        if (!strcmp(c->act, "shot")) sscanf(rest, "%63s", c->name);
        else sscanf(rest, "%x %x %x", &c->a, &c->b, &c->c);
        g_ncmd++;
    }
    fclose(f);
    lg("# %d commands\n", g_ncmd);
}

/* game addresses */
#define G_RNG 0x164D520
#define G_SCENE 0x17CE8B0
#define G_GMFLAGS 0x164D0B4
#define G_DEMOIDX 0x164D0BC
#define G_DEMOFR 0x164D0C0
#define G_GMFRAME 0x164D2C8
#define G_STAGE 0x164D2CC
#define G_LOADING 0x160F540
#define G_STATS 0x160F510
#define G_RANK 0x164D334
#define G_INPUT 0x164D52C
#define G_PLAYER 0x17D5EF8
#define G_BULLETS 0x160F3C8
#define G_SPELL 0x4EA670
#define G_ENEMIES 0x577F20
#define G_BOSS 0xF54CC0
#define G_PAUSE 0x164D0BA

static int in_demo(void) { return U32(G_SCENE) == 2 && (U32(G_GMFLAGS) & 2); }

static void save_shot(void *dev, const char *name);
static int g_dl_mode;
static FILE *g_draw;

static void scene_track(void) {
    if (!g_in_base && U32(G_SCENE) == 1) { g_in_base = g_present_n; lg("# menu frame base = present %u\n", g_in_base); }
    if (U32(G_SCENE) != g_scene_prev) { lg("# scene %u -> %u at present %u\n", g_scene_prev, U32(G_SCENE), g_present_n); g_scene_prev = U32(G_SCENE); g_scene_n = 0; }
    g_scene_n++;
}

static void run_cmds(void *dev) {
    if (g_nodik && U32(0x17CE768)) { lg("# zeroing DI keyboard ptr %08x at present %u (gks calls %d)\n", U32(0x17CE768), g_present_n, g_gks_calls); U32(0x17CE768) = 0; }
    for (int i = 0; i < g_ncmd; i++) {
        Cmd *c = &g_cmds[i];
        if (c->done) continue;
        int hit = 0;
        if (c->ctype == C_PRESENT) hit = (int)g_present_n == c->f;
        else if (c->ctype == C_MENU) hit = g_in_base && (int)(g_present_n - g_in_base + 1) == c->f;
        else if (c->ctype == C_SCENE) hit = (int)U32(G_SCENE) == c->k && (int)g_scene_n == c->f;
        else hit = in_demo() && U8(G_DEMOIDX) == c->k && (int)U32(G_GMFRAME) == c->f;
        if (!hit) continue;
        c->done = 1;
        lg("# cmd %d %s at present %u gmframe %u\n", i, c->act, g_present_n, U32(G_GMFRAME));
        if (!strcmp(c->act, "shot") && dev) save_shot(dev, c->name);
        else if (!strcmp(c->act, "key")) { g_keys[c->a & 255] = 0x80; g_key_until[c->a & 255] = g_present_n + (c->b ? c->b : 2); }
        else if (!strcmp(c->act, "poke1")) U8(c->a) = (uint8_t)c->b;
        else if (!strcmp(c->act, "poke2")) U16(c->a) = (uint16_t)c->b;
        else if (!strcmp(c->act, "poke4")) U32(c->a) = c->b;
        else if (!strcmp(c->act, "peek")) { char hb[1024]; int n = 0; for (uint32_t i = 0; i < c->b && i < 300; i++) n += snprintf(hb + n, sizeof hb - n, "%02x%s", U8(c->a + i), (i & 3) == 3 ? " " : ""); lg("# peek %08x: %s\n", c->a, hb); }
        else if (!strcmp(c->act, "nodik")) g_nodik = 1;
        else if (!strcmp(c->act, "effscan")) g_effscan_until = g_present_n + c->a;
        else if (!strcmp(c->act, "dl")) { g_dl_mode = c->a; lg("# drawlog mode %u at present %u\n", c->a, g_present_n); }
        else if (!strcmp(c->act, "quit")) { fflush(NULL); if (g_draw) fclose(g_draw); ExitProcess(0); }
    }
    if ((int)g_present_n < g_effscan_until) {
        char buf[2048]; int n = 0;
        n += snprintf(buf + n, sizeof buf - n, "# eff p%u gm%u:", g_present_n, U32(G_GMFRAME));
        for (int i = 0; i < 653 && n < 1900; i++) {
            uint32_t e = 0x4ECE60 + 0x1C + i * 0x360;
            if (!U8(e + 0x350)) continue;
            uint8_t t = U8(e + 0x351);
            if (!((t >= 4 && t <= 12) || t == 59)) continue;
            n += snprintf(buf + n, sizeof buf - n, " [%d t%u age%d spr%d ip%08x a%02x]", i, t, (int)U32(e + 0x340), (int)U32(e + 0x214), U32(e + 0x220), U8(e + 0x1F3));
        }
        lg("%s\n", buf);
    }
    for (int k = 0; k < 256; k++) if (g_keys[k] && (int)g_present_n >= g_key_until[k]) g_keys[k] = 0;
}

static BOOL WINAPI h_GetKeyboardState(PBYTE s) {
    BOOL r = o_GetKeyboardState(s);
    g_gks_calls++;
    for (int k = 0; k < 256; k++) s[k] = g_keys[k] ? 0x80 : 0;
    return r;
}

/* ---- callbacks from inline hooks (no FPU use!) ---- */
static void __cdecl cb_rng(void) {
    g_rng_calls++;
    if (GetCurrentThreadId() == g_main_tid) g_rng_main++;
    else {
        static uint16_t seen[8]; static int nseen;
        uint16_t cw = fpu_cw();
        for (int i = 0; i < nseen; i++) if (seen[i] == cw) return;
        if (nseen < 8) seen[nseen++] = cw;
        lg("# rng call from thread %lu cw=%04x\n", GetCurrentThreadId(), cw);
    }
}

static void __cdecl cb_stagestart(void) {
    lg("# StageStart thread %lu (main %lu) cw=%04x stage=%u rng=%04x calls=%u\n",
       GetCurrentThreadId(), g_main_tid, fpu_cw(), U32(G_STAGE), U16(G_RNG), g_rng_calls);
}

static uint32_t g_pass_rows;
static int g_pass;
static void emit_row(void) {
    if (U32(G_SCENE) != 2 || (!in_demo() && !g_trace_all)) return;
    uint32_t st = U32(G_STATS);
    int en = 0;
    for (int i = 0; i < 480; i++) if (U8(G_ENEMIES + 0x53D0 + i * 0x53D0 + 0x3324) & 1) en++;
    int boss = 0;
    for (int i = 0; i < 4; i++) if (U32(G_BOSS + 4 * i)) boss |= 1 << i;
    fprintf(g_trace, "%u %u %u %u %u %u %u %04x %04x %u %u %08x %08x %u %u %u %u %08x %08x %08x %d %d %u %u %d %u %x %u %u %d\n",
            g_calc_n, g_present_n, U8(G_DEMOIDX), U32(G_STAGE), U32(G_LOADING), U32(G_GMFRAME), U32(G_DEMOFR),
            U16(G_INPUT), U16(G_RNG), U32(G_RNG + 4), g_rng_calls,
            U32(G_PLAYER + 0x2B4), U32(G_PLAYER + 0x2B8), U8(G_PLAYER),
            U32(st + 8), U32(st + 0xC), U32(st + 0x30),
            U32(st + 0x74), U32(st + 0x80), U32(st + 0x98), (int16_t)U16(st + 0x22), (int)U32(st + 0x3C),
            U32(G_BULLETS), en, (int)U32(G_RANK), U8(G_SPELL) & 1, boss, U8(G_PAUSE), U32(G_GMFLAGS), g_pass);
    fflush(g_trace);
}

static void __cdecl cb_passstart(void) {
    if (g_pass > 0) { g_pass_rows++; emit_row(); }
    g_pass++;
}

static void __cdecl cb_postcalc(void) {
    uint16_t cw = fpu_cw();
    g_calc_n++;
    g_rng_at_calc = g_rng_main;
    if (cw != g_last_cw_calc) { lg("# Frame_Run post-calc cw=%04x at calc %u present %u\n", cw, g_calc_n, g_present_n); g_last_cw_calc = cw; }
    g_pass--;
    emit_row();
    g_pass = 0;
}

/* ---- inline hook ---- */
static void inline_hook(uint32_t addr, void (*cb)(void), int len) {
    uint8_t *stub = VirtualAlloc(NULL, 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE), *p = stub;
    *p++ = 0x60; *p++ = 0x9C;
    *p++ = 0xE8; *(int32_t *)p = (int32_t)((uint8_t *)cb - (p + 4)); p += 4;
    *p++ = 0x9D; *p++ = 0x61;
    memcpy(p, (void *)addr, len); p += len;
    *p++ = 0xE9; *(int32_t *)p = (int32_t)((addr + len) - (uint32_t)(p + 4)); p += 4;
    DWORD old;
    VirtualProtect((void *)addr, len, PAGE_EXECUTE_READWRITE, &old);
    uint8_t *t = (uint8_t *)addr;
    t[0] = 0xE9; *(int32_t *)(t + 1) = (int32_t)((uint32_t)stub - (addr + 5));
    for (int i = 5; i < len; i++) t[i] = 0x90;
    VirtualProtect((void *)addr, len, old, &old);
    FlushInstructionCache(GetCurrentProcess(), (void *)addr, len);
}

static void inline_hook_sp(uint32_t addr, void (*cb)(uint32_t *), int len) {
    uint8_t *stub = VirtualAlloc(NULL, 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE), *p = stub;
    *p++ = 0x60; *p++ = 0x9C; *p++ = 0x54;
    *p++ = 0xE8; *(int32_t *)p = (int32_t)((uint8_t *)cb - (p + 4)); p += 4;
    *p++ = 0x83; *p++ = 0xC4; *p++ = 0x04;
    *p++ = 0x9D; *p++ = 0x61;
    memcpy(p, (void *)addr, len); p += len;
    *p++ = 0xE9; *(int32_t *)p = (int32_t)((addr + len) - (uint32_t)(p + 4)); p += 4;
    DWORD old;
    VirtualProtect((void *)addr, len, PAGE_EXECUTE_READWRITE, &old);
    uint8_t *t = (uint8_t *)addr;
    t[0] = 0xE9; *(int32_t *)(t + 1) = (int32_t)((uint32_t)stub - (addr + 5));
    for (int i = 5; i < len; i++) t[i] = 0x90;
    VirtualProtect((void *)addr, len, old, &old);
    FlushInstructionCache(GetCurrentProcess(), (void *)addr, len);
}

static void patch_vtbl(void *obj, int idx, void *fn, void **orig) {
    void **vt = *(void ***)obj; DWORD old;
    if (vt[idx] == fn) return;
    VirtualProtect(&vt[idx], 4, PAGE_READWRITE, &old);
    *orig = vt[idx]; vt[idx] = fn;
    VirtualProtect(&vt[idx], 4, old, &old);
}

#include "drawlog.inc"

/* ---- screenshots ---- */
static void save_shot(void *dev, const char *name) {
    void *surf = NULL;
    typedef HRESULT(WINAPI *GBB)(void *, UINT, int, void **);
    typedef HRESULT(WINAPI *GD)(void *, uint32_t *);
    typedef HRESULT(WINAPI *LR)(void *, void *, const RECT *, DWORD);
    typedef HRESULT(WINAPI *UR)(void *);
    typedef ULONG(WINAPI *RL)(void *);
    void **vt = *(void ***)dev;
    if (((GBB)vt[16])(dev, 0, 0, &surf) < 0 || !surf) { lg("# GetBackBuffer failed\n"); return; }
    void **sv = *(void ***)surf;
    uint32_t desc[8]; ((GD)sv[8])(surf, desc);
    struct { int pitch; uint8_t *bits; } lr;
    if (((LR)sv[9])(surf, &lr, NULL, 0x10) < 0) { lg("# LockRect failed fmt=%u\n", desc[0]); ((RL)sv[2])(surf); return; }
    uint32_t w = desc[6], h = desc[7], fmt = desc[0];
    char fn[128]; snprintf(fn, sizeof fn, "shots/%s.ppm", name);
    FILE *f = fopen(fn, "wb");
    if (f) {
        fprintf(f, "P6\n%u %u\n255\n", w, h);
        uint8_t *row = malloc(w * 3);
        for (uint32_t y = 0; y < h; y++) {
            uint8_t *s = lr.bits + y * lr.pitch;
            for (uint32_t x = 0; x < w; x++) {
                if (fmt == 22 || fmt == 21) { row[x*3] = s[x*4+2]; row[x*3+1] = s[x*4+1]; row[x*3+2] = s[x*4]; }
                else { uint16_t v = ((uint16_t *)s)[x];
                    if (fmt == 23) { row[x*3] = (v >> 11) << 3; row[x*3+1] = ((v >> 5) & 63) << 2; row[x*3+2] = (v & 31) << 3; }
                    else { row[x*3] = ((v >> 10) & 31) << 3; row[x*3+1] = ((v >> 5) & 31) << 3; row[x*3+2] = (v & 31) << 3; } }
            }
            fwrite(row, 1, w * 3, f);
        }
        free(row); fclose(f);
    }
    ((UR)sv[10])(surf); ((RL)sv[2])(surf);
    lg("# shot %s %ux%u fmt %u\n", name, w, h, fmt);
}

static HRESULT WINAPI h_Present(void *dev, const RECT *a, const RECT *b, HWND w, const void *d) {
    uint16_t cw = fpu_cw();
    g_present_n++;
    if (cw != g_last_cw_present) { lg("# Present cw=%04x at present %u\n", cw, g_present_n); g_last_cw_present = cw; }
    if (g_rng_main != g_rng_at_calc) { g_draw_rng_frames++; if (g_draw_rng_frames < 20) lg("# draw phase used %u main-thread rng calls at present %u scene %u\n", g_rng_main - g_rng_at_calc, g_present_n, U32(G_SCENE)); }
    scene_track();
    dl_present();
    run_cmds(dev);
    input_apply();
    return o_Present(dev, a, b, w, d);
}

static HRESULT WINAPI h_Reset(void *dev, void *pp) {
    ((uint32_t *)pp)[10] |= 1;
    HRESULT r = o_Reset(dev, pp);
    if (r >= 0) dl_read_state(dev);
    lg("# Reset hr=%08lx at present %u\n", r, g_present_n);
    return r;
}

static HRESULT WINAPI h_CreateDevice(void *d3d, UINT ad, int type, HWND hw, DWORD flags, void *pp, void **out) {
    uint32_t *p = pp;
    lg("# CreateDevice before: cw=%04x flags=%08lx bb %ux%u fmt %u cnt %u ms %u swap %u windowed %u flags %x\n",
       fpu_cw(), flags, p[0], p[1], p[2], p[3], p[4], p[5], p[7], p[10]);
    p[10] |= 1;
    HRESULT r = o_CreateDevice(d3d, ad, type, hw, flags, pp, out);
    lg("# CreateDevice after: hr=%08lx cw=%04x\n", r, fpu_cw());
    if (r >= 0 && *out) {
        patch_vtbl(*out, 15, h_Present, (void **)&o_Present);
        patch_vtbl(*out, 14, h_Reset, (void **)&o_Reset);
        dl_hook_device(*out);
    }
    return r;
}

void *WINAPI Direct3DCreate8(UINT v) {
    lg("# Direct3DCreate8 cw=%04x\n", fpu_cw());
    void *d3d = ((PFN_Create8)GetProcAddress(g_real, "Direct3DCreate8"))(v);
    if (d3d) patch_vtbl(d3d, 15, h_CreateDevice, (void **)&o_CreateDevice);
    return d3d;
}

static void hook_iat(const char *dll, const char *fn, void *h, void **orig) {
    uint8_t *base = (uint8_t *)GetModuleHandleA(NULL);
    IMAGE_NT_HEADERS *nt = (IMAGE_NT_HEADERS *)(base + ((IMAGE_DOS_HEADER *)base)->e_lfanew);
    IMAGE_IMPORT_DESCRIPTOR *d = (void *)(base + nt->OptionalHeader.DataDirectory[1].VirtualAddress);
    for (; d->Name; d++) {
        if (_stricmp((char *)base + d->Name, dll)) continue;
        IMAGE_THUNK_DATA *on = (void *)(base + d->OriginalFirstThunk), *ft = (void *)(base + d->FirstThunk);
        for (; on->u1.AddressOfData; on++, ft++) {
            if (on->u1.Ordinal & IMAGE_ORDINAL_FLAG) continue;
            IMAGE_IMPORT_BY_NAME *ibn = (void *)(base + on->u1.AddressOfData);
            if (strcmp((char *)ibn->Name, fn)) continue;
            DWORD old; VirtualProtect(&ft->u1.Function, 4, PAGE_READWRITE, &old);
            *orig = (void *)ft->u1.Function; ft->u1.Function = (DWORD)h;
            VirtualProtect(&ft->u1.Function, 4, old, &old);
            return;
        }
    }
}

BOOL WINAPI DllMain(HINSTANCE inst, DWORD reason, LPVOID res) {
    if (reason != DLL_PROCESS_ATTACH) return TRUE;
    char path[MAX_PATH];
    GetSystemDirectoryA(path, MAX_PATH); strcat(path, "\\d3d8.dll");
    g_real = LoadLibraryA(path);
    CreateDirectoryA("shots", NULL);
    g_log = fopen("probe_log.txt", "w");
    g_trace = fopen("probe_trace.txt", "w");
    g_main_tid = GetCurrentThreadId();
    lg("# probe loaded, real d3d8=%p, DllMain cw=%04x tid %lu\n", g_real, fpu_cw(), g_main_tid);
    load_cmds();
    load_input();
    if (g_inmask) g_nodik = 1;
    g_draw = fopen("probe_draw.txt", "w");
    if (g_draw) setvbuf(g_draw, NULL, _IOFBF, 1 << 22);
    inline_hook(0x43ECC0, cb_rng, 6);
    inline_hook(0x45D790, cb_postcalc, 6);
    inline_hook(0x43ABD7, cb_stagestart, 6);
    inline_hook(0x43CA65, cb_passstart, 7);
    inline_hook_sp(0x465860, cb_anm_preload, 6);
    inline_hook_sp(0x4667C0, cb_surf_load, 6);
    hook_iat("USER32.dll", "GetKeyboardState", h_GetKeyboardState, (void **)&o_GetKeyboardState);
    return TRUE;
}
