/* ---- draw log (autocmp): shadow D3D8 state, write effective per-quad records ---- */
#include <stdarg.h>
#define ANMMGR U32(0x18BDC90)

static FILE *g_draw;
static int g_dl_mode;                 /* 0 off, 1 every frame, 2 demo frames only */
static char *g_fb; static size_t g_fbn, g_fbcap;
static uint32_t g_rs[256], g_tss[32];
static float g_mw[16], g_mv[16], g_mp[16], g_mt[16];
static uint32_t g_vp[6];              /* x y w h, minz/maxz as float bits */
static void *g_tex, *g_vb; static uint32_t g_vb_stride, g_fvf;
static char g_last_T[96], g_last_S[256], g_last_V[96];
static uint32_t g_quads_frame;
static char g_anm_name[25][64], g_surf_name[32][64];

static void fbw(const char *fmt, ...) {
    char tmp[1024]; va_list ap; va_start(ap, fmt);
    int n = vsnprintf(tmp, sizeof tmp, fmt, ap); va_end(ap);
    if (n < 0) return; if (n > (int)sizeof tmp - 1) n = sizeof tmp - 1;
    if (g_fbn + n + 1 > g_fbcap) { g_fbcap = (g_fbcap + n + 1) * 2; g_fb = realloc(g_fb, g_fbcap); }
    memcpy(g_fb + g_fbn, tmp, n); g_fbn += n;
}

static int dl_active(void) {
    if (!g_draw || !g_dl_mode) return 0;
    if (g_dl_mode == 2) return in_demo();
    return 1;
}

/* number formatting: fixed decimals, trailing zeros stripped */
static char *fnum(char *p, double v, int dec) {
    char t[48];
    if (v != v) strcpy(t, "nan");
    else if (v > 1e300 || v < -1e300) strcpy(t, v > 0 ? "inf" : "-inf");
    else snprintf(t, sizeof t, "%.*f", dec, v);
    char *e = t + strlen(t);
    if (strchr(t, '.')) { while (e[-1] == '0') e--; if (e[-1] == '.') e--; }
    *e = 0;
    if (!strcmp(t, "-0")) strcpy(t, "0");
    size_t n = strlen(t); *p++ = ' '; memcpy(p, t, n); return p + n;
}

static const char *basename_of(const char *s) {
    const char *b = s;
    for (const char *p = s; *p; p++) if (*p == '/' || *p == '\\') b = p + 1;
    return b;
}

/* hook callbacks (inline, no FPU) */
static void __cdecl cb_anm_preload(uint32_t *sp) {
    uint32_t slot = sp[10]; const char *name = (const char *)sp[11];
    if (slot < 25 && name) { strncpy(g_anm_name[slot], basename_of(name), 63); g_anm_name[slot][63] = 0; }
}
static void __cdecl cb_surf_load(uint32_t *sp) {
    uint32_t slot = sp[10]; const char *name = (const char *)sp[11];
    if (slot < 32 && name) { strncpy(g_surf_name[slot], basename_of(name), 63); g_surf_name[slot][63] = 0; }
}

static void tex_label(void *tex, char *out, size_t n) {
    if (!tex) { snprintf(out, n, "-"); return; }
    for (int s = 0; s < 25; s++) {
        uint32_t f = ANMMGR + 0x24 + s * 0x1C;
        uint32_t data = U32(f + 4), cnt = U32(f + 8), tt = U32(f + 0x14);
        if (!data || !tt || cnt > 64) continue;
        uint32_t ent = data;
        for (uint32_t e = 0; e < cnt; e++) {
            if (U32(tt + e * 0xC) == (uint32_t)tex) {
                snprintf(out, n, "%s#%u %u %u", g_anm_name[s][0] ? g_anm_name[s] : "?", e, U32(ent + 0xC), U32(ent + 0x10));
                return;
            }
            if (!U32(ent + 0x38)) break;
            ent += U32(ent + 0x38);
        }
    }
    snprintf(out, n, "?%08x 0 0", (uint32_t)tex);
}

static const char *blend_name(uint32_t b) {
    static const char *n[] = {"?", "zero", "one", "srccolor", "invsrccolor", "srcalpha", "invsrcalpha",
                              "destalpha", "invdestalpha", "destcolor", "invdestcolor", "srcalphasat"};
    return b < 12 ? n[b] : "?";
}
static const char *cmp_name(uint32_t f) {
    static const char *n[] = {"?", "never", "lt", "eq", "le", "gt", "ne", "ge", "always"};
    return f < 9 ? n[f] : "?";
}
static void combine_name(char *o, size_t n, uint32_t op, uint32_t a1, uint32_t a2) {
    const char *s1 = (a1 & 15) == 2 ? "tex" : "col", *s2 = (a2 & 15) == 2 ? "tex" : "col";
    switch (op) {
    case 1: snprintf(o, n, "off"); break;
    case 2: snprintf(o, n, "%s", s1); break;
    case 3: snprintf(o, n, "%s", s2); break;
    case 4: snprintf(o, n, "%s*%s", s1, s2); break;
    case 5: snprintf(o, n, "%s*%s*2", s1, s2); break;
    case 7: snprintf(o, n, "%s+%s", s1, s2); break;
    default: snprintf(o, n, "op%u", op);
    }
}

static void emit_state(void) {
    char t[96], s[256], v[96], c[24], a[24];
    if (g_tex) tex_label(g_tex, t, sizeof t); else strcpy(t, "-");
    if (strcmp(t, g_last_T)) { fbw("T %s\n", t); strcpy(g_last_T, t); }
    int n = 0;
    if (!g_rs[27]) n += snprintf(s + n, sizeof s - n, "blend=off");
    else if (g_rs[19] == 5 && g_rs[20] == 6) n += snprintf(s + n, sizeof s - n, "blend=alpha");
    else if (g_rs[19] == 5 && g_rs[20] == 2) n += snprintf(s + n, sizeof s - n, "blend=add");
    else n += snprintf(s + n, sizeof s - n, "blend=%s,%s", blend_name(g_rs[19]), blend_name(g_rs[20]));
    if (g_rs[27] && g_rs[171] != 1) n += snprintf(s + n, sizeof s - n, ",op%u", g_rs[171]);
    if (g_rs[15]) n += snprintf(s + n, sizeof s - n, " atest=%s%u", cmp_name(g_rs[25]), g_rs[24]);
    else n += snprintf(s + n, sizeof s - n, " atest=off");
    n += snprintf(s + n, sizeof s - n, " ztest=%s zwrite=%u", g_rs[7] ? cmp_name(g_rs[23]) : "off", g_rs[14] ? 1 : 0);
    combine_name(c, sizeof c, g_tss[1], g_tss[2], g_tss[3]);
    combine_name(a, sizeof a, g_tss[4], g_tss[5], g_tss[6]);
    n += snprintf(s + n, sizeof s - n, " c=%s a=%s", c, a);
    if (g_rs[28] && g_rs[140]) {
        float fs, fe; memcpy(&fs, &g_rs[36], 4); memcpy(&fe, &g_rs[37], 4);
        n += snprintf(s + n, sizeof s - n, " fog=%08x,%g,%g", g_rs[34], fs, fe);
    }
    if (strcmp(s, g_last_S)) { fbw("S %s\n", s); strcpy(g_last_S, s); }
    float mn, mx; memcpy(&mn, &g_vp[4], 4); memcpy(&mx, &g_vp[5], 4);
    if (mn == 0.f && mx == 1.f) snprintf(v, sizeof v, "%u %u %u %u", g_vp[0], g_vp[1], g_vp[2], g_vp[3]);
    else snprintf(v, sizeof v, "%u %u %u %u %g %g", g_vp[0], g_vp[1], g_vp[2], g_vp[3], mn, mx);
    if (strcmp(v, g_last_V)) { fbw("V %s\n", v); strcpy(g_last_V, v); }
}

typedef struct { double x, y, z, w, u, v; uint32_t c; int has_uv; } DV;

static void mat_mul(const float *a, const float *b, double *o) {
    for (int i = 0; i < 4; i++) for (int j = 0; j < 4; j++) {
        double s = 0; for (int k = 0; k < 4; k++) s += (double)a[i * 4 + k] * b[k * 4 + j]; o[i * 4 + j] = s;
    }
}

static double g_wvp[16];
static void build_wvp(void) {
    double wv[16]; float wvf[16];
    mat_mul(g_mw, g_mv, wv);
    for (int i = 0; i < 16; i++) wvf[i] = (float)wv[i];
    mat_mul(wvf, g_mp, g_wvp);
}

static uint32_t eff_colour(uint32_t diffuse) {
    uint32_t tf = g_rs[60], c = diffuse;
    if ((g_tss[3] & 15) == 3) c = (c & 0xFF000000) | (tf & 0x00FFFFFF);
    if ((g_tss[6] & 15) == 3) c = (c & 0x00FFFFFF) | (tf & 0xFF000000);
    return c;
}

static void decode_vertex(const uint8_t *p, uint32_t fvf, DV *o) {
    int pos = fvf & 0xE; const float *f = (const float *)p;
    int off;
    if (pos == 4) { o->x = f[0]; o->y = f[1]; o->z = f[2]; o->w = f[3]; off = 16; }
    else {
        double x = f[0], y = f[1], z = f[2], *m = g_wvp;
        double cx = x * m[0] + y * m[4] + z * m[8] + m[12];
        double cy = x * m[1] + y * m[5] + z * m[9] + m[13];
        double cz = x * m[2] + y * m[6] + z * m[10] + m[14];
        double cw = x * m[3] + y * m[7] + z * m[11] + m[15];
        float mn, mx; memcpy(&mn, &g_vp[4], 4); memcpy(&mx, &g_vp[5], 4);
        double iw = cw != 0 ? 1.0 / cw : 0;
        o->x = g_vp[0] + (1 + cx * iw) * g_vp[2] * 0.5;
        o->y = g_vp[1] + (1 - cy * iw) * g_vp[3] * 0.5;
        o->z = mn + cz * iw * (mx - mn);
        o->w = iw;
        off = 12;
    }
    if (fvf & 0x10) off += 12;
    if (fvf & 0x20) off += 4;
    uint32_t d = 0xFFFFFFFF;
    if (fvf & 0x40) { d = *(const uint32_t *)(p + off); off += 4; }
    if (fvf & 0x80) off += 4;
    o->c = eff_colour(d);
    o->has_uv = ((fvf >> 8) & 15) > 0;
    if (o->has_uv) {
        double u = ((const float *)(p + off))[0], v = ((const float *)(p + off))[1];
        if (pos != 4 && g_tss[24] == 2) {
            double u2 = u * g_mt[0] + v * g_mt[4] + g_mt[8], v2 = u * g_mt[1] + v * g_mt[5] + g_mt[9];
            u = u2; v = v2;
        }
        o->u = u; o->v = v;
    } else o->u = o->v = 0;
}

static char *put_v(char *p, const DV *v) {
    p = fnum(p, v->x, 2); p = fnum(p, v->y, 2); p = fnum(p, v->z, 4); p = fnum(p, v->w, 4);
    if (v->has_uv) { p = fnum(p, v->u, 5); p = fnum(p, v->v, 5); } else { *p++ = ' '; *p++ = '-'; *p++ = ' '; *p++ = '-'; }
    p += sprintf(p, " %08x", v->c);
    return p;
}

static void emit_quad(const DV *q) {
    char line[512], *p = line;
    g_quads_frame++;
    int rect = q[0].y == q[1].y && q[2].y == q[3].y && q[0].x == q[2].x && q[1].x == q[3].x &&
               q[0].z == q[1].z && q[0].z == q[2].z && q[0].z == q[3].z &&
               q[0].w == 1 && q[1].w == 1 && q[2].w == 1 && q[3].w == 1 &&
               q[0].u == q[2].u && q[1].u == q[3].u && q[0].v == q[1].v && q[2].v == q[3].v &&
               q[0].c == q[1].c && q[0].c == q[2].c && q[0].c == q[3].c && q[0].has_uv;
    if (rect) {
        *p++ = 'R';
        p = fnum(p, q[0].x, 2); p = fnum(p, q[0].y, 2); p = fnum(p, q[3].x, 2); p = fnum(p, q[3].y, 2);
        p = fnum(p, q[0].u, 5); p = fnum(p, q[0].v, 5); p = fnum(p, q[3].u, 5); p = fnum(p, q[3].v, 5);
        p += sprintf(p, " %08x", q[0].c);
        if (q[0].z != 0) p = fnum(p, q[0].z, 4);
    } else {
        *p++ = 'Q';
        for (int i = 0; i < 4; i++) p = put_v(p, &q[i]);
    }
    *p++ = '\n'; *p = 0;
    fbw("%s", line);
}

static void emit_poly(const char *kind, const DV *v, int n) {
    fbw("P %s %d", kind, n);
    for (int i = 0; i < n; i++) { char b[160], *p = put_v(b, &v[i]); *p = 0; fbw("%s", b); }
    fbw("\n");
}

static void dl_draw(uint32_t type, uint32_t count, const uint8_t *data, uint32_t stride) {
    if (!dl_active() || !data) return;
    uint32_t nv = type == 4 ? count * 3 : type == 5 || type == 6 ? count + 2 : type == 3 ? count * 2 : type == 2 ? count * 2 : count;
    if (nv > 4096) nv = 4096;
    static DV v[4096];
    if ((g_fvf & 0xE) != 4) build_wvp();
    emit_state();
    for (uint32_t i = 0; i < nv; i++) decode_vertex(data + i * stride, g_fvf, &v[i]);
    if (type == 5 && nv == 4) { emit_quad(v); return; }
    if (type == 4) {
        uint32_t i = 0;
        for (; i + 6 <= nv; i += 6) {
            if (!memcmp(data + (i + 3) * stride, data + (i + 1) * stride, stride) &&
                !memcmp(data + (i + 4) * stride, data + (i + 2) * stride, stride)) {
                DV q[4] = {v[i], v[i + 1], v[i + 2], v[i + 5]}; emit_quad(q);
            } else emit_poly("list", &v[i], 6);
        }
        if (i < nv) emit_poly("list", &v[i], nv - i);
        return;
    }
    emit_poly(type == 5 ? "strip" : type == 6 ? "fan" : type == 1 ? "points" : "lines", v, nv);
}

/* ---- device method hooks ---- */
typedef HRESULT(WINAPI *PFN_SRS)(void *, uint32_t, uint32_t);
typedef HRESULT(WINAPI *PFN_STSS)(void *, uint32_t, uint32_t, uint32_t);
typedef HRESULT(WINAPI *PFN_STex)(void *, uint32_t, void *);
typedef HRESULT(WINAPI *PFN_STr)(void *, uint32_t, const float *);
typedef HRESULT(WINAPI *PFN_SVp)(void *, const uint32_t *);
typedef HRESULT(WINAPI *PFN_SVS)(void *, uint32_t);
typedef HRESULT(WINAPI *PFN_SSS)(void *, uint32_t, void *, uint32_t);
typedef HRESULT(WINAPI *PFN_DP)(void *, uint32_t, uint32_t, uint32_t);
typedef HRESULT(WINAPI *PFN_DPUP)(void *, uint32_t, uint32_t, const void *, uint32_t);
typedef HRESULT(WINAPI *PFN_CR)(void *, void *, const RECT *, uint32_t, void *, const POINT *);
typedef HRESULT(WINAPI *PFN_Clr)(void *, uint32_t, const void *, uint32_t, uint32_t, float, uint32_t);
typedef HRESULT(WINAPI *PFN_GRS)(void *, uint32_t, uint32_t *);
typedef HRESULT(WINAPI *PFN_GTSS)(void *, uint32_t, uint32_t, uint32_t *);
typedef HRESULT(WINAPI *PFN_GTr)(void *, uint32_t, float *);
typedef HRESULT(WINAPI *PFN_GVp)(void *, uint32_t *);
typedef HRESULT(WINAPI *PFN_DIP)(void *, uint32_t, uint32_t, uint32_t, uint32_t, uint32_t);
static PFN_SRS o_SRS; static PFN_STSS o_STSS; static PFN_STex o_STex; static PFN_STr o_STr; static PFN_SVp o_SVp;
static PFN_SVS o_SVS; static PFN_SSS o_SSS; static PFN_DP o_DP; static PFN_DPUP o_DPUP; static PFN_CR o_CR;
static PFN_Clr o_Clr; static PFN_DIP o_DIP; static void *o_DIPUP;

static HRESULT WINAPI h_SRS(void *d, uint32_t s, uint32_t v) { if (s < 256) g_rs[s] = v; return o_SRS(d, s, v); }
static HRESULT WINAPI h_STSS(void *d, uint32_t st, uint32_t s, uint32_t v) { if (st == 0 && s < 32) g_tss[s] = v; return o_STSS(d, st, s, v); }
static HRESULT WINAPI h_STex(void *d, uint32_t st, void *t) { if (st == 0) g_tex = t; return o_STex(d, st, t); }
static HRESULT WINAPI h_STr(void *d, uint32_t s, const float *m) {
    float *dst = s == 256 ? g_mw : s == 2 ? g_mv : s == 3 ? g_mp : s == 16 ? g_mt : NULL;
    if (dst && m) memcpy(dst, m, 64);
    return o_STr(d, s, m);
}
static HRESULT WINAPI h_SVp(void *d, const uint32_t *vp) { if (vp) memcpy(g_vp, vp, 24); return o_SVp(d, vp); }
static HRESULT WINAPI h_SVS(void *d, uint32_t h) { g_fvf = h; return o_SVS(d, h); }
static HRESULT WINAPI h_SSS(void *d, uint32_t n, void *vb, uint32_t stride) { if (n == 0) { g_vb = vb; g_vb_stride = stride; } return o_SSS(d, n, vb, stride); }
static HRESULT WINAPI h_DP(void *d, uint32_t type, uint32_t start, uint32_t count) {
    if (dl_active()) {
        if (g_vb && (uint32_t)g_vb == U32(ANMMGR + 0x24CC) && g_vb_stride == 0x14)
            dl_draw(type, count, (const uint8_t *)(ANMMGR + 0x24D0) + start * 0x14, 0x14);
        else {
            typedef HRESULT(WINAPI *LK)(void *, uint32_t, uint32_t, uint8_t **, uint32_t);
            typedef HRESULT(WINAPI *UL)(void *);
            uint8_t *p = NULL; void **vt = g_vb ? *(void ***)g_vb : NULL;
            if (vt && ((LK)vt[11])(g_vb, 0, 0, &p, 0x10) >= 0 && p) { dl_draw(type, count, p + start * g_vb_stride, g_vb_stride); ((UL)vt[12])(g_vb); }
            else fbw("# DrawPrimitive unreadable vb\n");
        }
    }
    return o_DP(d, type, start, count);
}
static HRESULT WINAPI h_DPUP(void *d, uint32_t type, uint32_t count, const void *data, uint32_t stride) {
    dl_draw(type, count, data, stride);
    return o_DPUP(d, type, count, data, stride);
}
static HRESULT WINAPI h_DIP(void *d, uint32_t a, uint32_t b, uint32_t c, uint32_t e, uint32_t f) {
    if (dl_active()) fbw("# DrawIndexedPrimitive not decoded\n");
    return o_DIP(d, a, b, c, e, f);
}

static void surf_label(void *dev, void *surf, char *out, size_t n) {
    typedef HRESULT(WINAPI *GBB)(void *, UINT, int, void **);
    typedef ULONG(WINAPI *RL)(void *);
    void *bb = NULL; void **vt = *(void ***)dev;
    if (((GBB)vt[16])(dev, 0, 0, &bb) >= 0 && bb) { ((RL)(*(void ***)bb)[2])(bb); if (bb == surf) { snprintf(out, n, "screen"); return; } }
    for (int s = 0; s < 32; s++) {
        if (U32(ANMMGR + 0x2038 + s * 4) == (uint32_t)surf || U32(ANMMGR + 0x20B8 + s * 4) == (uint32_t)surf) {
            snprintf(out, n, "%s", g_surf_name[s][0] ? g_surf_name[s] : "surf?"); return;
        }
    }
    typedef HRESULT(WINAPI *GSL)(void *, UINT, void **);
    for (int s = 0; s < 25; s++) {
        uint32_t f = ANMMGR + 0x24 + s * 0x1C, cnt = U32(f + 8), tt = U32(f + 0x14);
        if (!U32(f + 4) || !tt || cnt > 64) continue;
        for (uint32_t e = 0; e < cnt; e++) {
            void *t = (void *)U32(tt + e * 0xC), *sl = NULL;
            if (!t) continue;
            if (((GSL)(*(void ***)t)[15])(t, 0, &sl) >= 0 && sl) {
                ((RL)(*(void ***)sl)[2])(sl);
                if (sl == surf) { snprintf(out, n, "%s#%u", g_anm_name[s], e); return; }
            }
        }
    }
    snprintf(out, n, "?");
}

static HRESULT WINAPI h_CR(void *d, void *src, const RECT *r, uint32_t n, void *dst, const POINT *pt) {
    if (dl_active()) {
        char sl[80], dl[80];
        surf_label(d, src, sl, sizeof sl); surf_label(d, dst, dl, sizeof dl);
        if (strcmp(sl, "screen") && strcmp(dl, "screen")) return o_CR(d, src, r, n, dst, pt);
        if (!r || !n) fbw("K %s %s full\n", sl, dl);
        for (uint32_t i = 0; r && i < n; i++)
            fbw("K %s %s %ld %ld %ld %ld %ld %ld\n", sl, dl, r[i].left, r[i].top, r[i].right - r[i].left, r[i].bottom - r[i].top,
                pt ? pt[i].x : r[i].left, pt ? pt[i].y : r[i].top);
    }
    return o_CR(d, src, r, n, dst, pt);
}
static HRESULT WINAPI h_Clr(void *d, uint32_t n, const void *rects, uint32_t flags, uint32_t col, float z, uint32_t st) {
    if (dl_active()) {
        char v[96]; float mn, mx; memcpy(&mn, &g_vp[4], 4); memcpy(&mx, &g_vp[5], 4);
        if (mn == 0.f && mx == 1.f) snprintf(v, sizeof v, "%u %u %u %u", g_vp[0], g_vp[1], g_vp[2], g_vp[3]);
        else snprintf(v, sizeof v, "%u %u %u %u %g %g", g_vp[0], g_vp[1], g_vp[2], g_vp[3], mn, mx);
        if (strcmp(v, g_last_V)) { fbw("V %s\n", v); strcpy(g_last_V, v); }
        fbw("C %s%s %08x", flags & 1 ? "t" : "", flags & 2 ? "z" : "", col);
        const RECT *r = rects;
        for (uint32_t i = 0; r && i < n; i++) fbw(" %ld,%ld,%ld,%ld", r[i].left, r[i].top, r[i].right - r[i].left, r[i].bottom - r[i].top);
        fbw("\n");
    }
    return o_Clr(d, n, rects, flags, col, z, st);
}

static void dl_read_state(void *dev) {
    void **vt = *(void ***)dev;
    for (uint32_t s = 0; s < 256; s++) { uint32_t v = 0; if (((PFN_GRS)vt[51])(dev, s, &v) >= 0) g_rs[s] = v; }
    for (uint32_t s = 0; s < 32; s++) { uint32_t v = 0; if (((PFN_GTSS)vt[62])(dev, 0, s, &v) >= 0) g_tss[s] = v; }
    ((PFN_GTr)vt[38])(dev, 256, g_mw); ((PFN_GTr)vt[38])(dev, 2, g_mv); ((PFN_GTr)vt[38])(dev, 3, g_mp); ((PFN_GTr)vt[38])(dev, 16, g_mt);
    ((PFN_GVp)vt[41])(dev, g_vp);
    g_tex = NULL; g_vb = NULL; g_fvf = 0;
}

static void dl_hook_device(void *dev) {
    patch_vtbl(dev, 50, h_SRS, (void **)&o_SRS);
    patch_vtbl(dev, 63, h_STSS, (void **)&o_STSS);
    patch_vtbl(dev, 61, h_STex, (void **)&o_STex);
    patch_vtbl(dev, 37, h_STr, (void **)&o_STr);
    patch_vtbl(dev, 40, h_SVp, (void **)&o_SVp);
    patch_vtbl(dev, 76, h_SVS, (void **)&o_SVS);
    patch_vtbl(dev, 83, h_SSS, (void **)&o_SSS);
    patch_vtbl(dev, 70, h_DP, (void **)&o_DP);
    patch_vtbl(dev, 72, h_DPUP, (void **)&o_DPUP);
    patch_vtbl(dev, 71, h_DIP, (void **)&o_DIP);
    patch_vtbl(dev, 28, h_CR, (void **)&o_CR);
    patch_vtbl(dev, 36, h_Clr, (void **)&o_Clr);
    dl_read_state(dev);
}

static uint32_t g_dl_frames;
/* called at Present, before the real Present: flush this frame's records */
static void dl_present(void) {
    if (dl_active()) {
        fprintf(g_draw, "F %u %u %u %u %d %u %04x %u\n", g_present_n, U32(G_SCENE), g_scene_n, U32(G_GMFRAME),
                in_demo() ? (int)U8(G_DEMOIDX) : -1, in_demo() ? U32(G_DEMOFR) : 0, U16(0x164D528), g_quads_frame);
        if (g_fbn) fwrite(g_fb, 1, g_fbn, g_draw);
        g_dl_frames++;
        if ((g_dl_frames & 63) == 0) fflush(g_draw);
    }
    g_fbn = 0; g_quads_frame = 0;
    g_last_T[0] = g_last_S[0] = g_last_V[0] = 0;
}

/* ---- input-word script (probe_input.txt, same syntax as the port's TH08_INPUT) ---- */
static uint16_t *g_inmask; static uint32_t g_innum;
static const uint8_t g_bit_vk[16] = {0x5A, 0x58, 0x10, 0x1B, 0x26, 0x28, 0x25, 0x27, 0x11, 0x51, 0x53, 0x50, 0x0D, 0x44, 0x52, 0};
static void load_input(void) {
    FILE *f = fopen("probe_input.txt", "r");
    if (!f) return;
    static char buf[1 << 20]; size_t n = fread(buf, 1, sizeof buf - 1, f); buf[n] = 0; fclose(f);
    uint32_t cap = 1 << 16; g_inmask = calloc(cap, 2);
    char *p = buf;
    while (*p) {
        if (*p == '#') { while (*p && *p != '\n') p++; continue; }
        if (*p == ',' || *p == '\n' || *p == '\r' || *p == ' ' || *p == '\t') { p++; continue; }
        char *e; unsigned long fr = strtoul(p, &e, 0);
        if (e == p || *e != ':') { p++; continue; }
        unsigned long m = strtoul(e + 1, &p, 0);
        for (unsigned long i = 0; i < fr && g_innum < cap; i++) g_inmask[g_innum++] = (uint16_t)m;
    }
    lg("# input script: %u frames\n", g_innum);
}
/* script frame 1 = first presented frame of the main menu (scene 1); boot length varies by a frame or two */
static void input_apply(void) {
    if (!g_inmask) return;
    if (!g_in_base) return;
    uint32_t k = g_present_n + 2 - g_in_base;
    uint16_t m = k >= 1 && k <= g_innum ? g_inmask[k - 1] : 0;
    for (int b = 0; b < 15; b++) {
        uint8_t vk = g_bit_vk[b];
        if (m & (1 << b)) { g_keys[vk] = 0x80; g_key_until[vk] = g_present_n + 1; }
    }
}
