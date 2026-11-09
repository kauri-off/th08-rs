//! Which setting appears in which Settings section on Android, in display order; keys not listed are not shown.
package org.th08.port

// `state` keys give the section's one-line current state; `labelled` prefixes each value with its row label.
class Section(val id: String, val title: Int, val keys: List<String>, val state: List<String> = emptyList(), val labelled: Boolean = false)

const val SEC_APP = "app"
const val SEC_ADVANCED = "advanced"
const val SEC_DEV = "dev"
const val SEC_LANGUAGE = "language"

val SECTIONS = listOf(
    Section("controls", R.string.section_controls,
        listOf("touch", "touch_move", "touch_side", "touch_focus", "touch_autofire", "touch_sens", "touch_opacity"),
        listOf("touch_move", "touch_side")),
    Section("sound", R.string.section_sound, listOf("volume_master", "volume_bgm", "volume_se", "now_playing"),
        listOf("volume_master"), labelled = true),
    Section("picture", R.string.section_picture,
        listOf("render_scale", "scale_filter", "integer_scale", "brightness", "color_filter", "hitboxes"),
        listOf("render_scale", "scale_filter")),
    Section(SEC_LANGUAGE, R.string.section_language, listOf("language", "game_text"), listOf("language")),
    Section(SEC_APP, R.string.section_app, listOf("stats", "update_check")),
    Section(SEC_ADVANCED, R.string.section_advanced,
        listOf("input_display", "input_display_opacity", "screenshot_size", "screen_margin", "pause_unfocused")),
    Section(SEC_DEV, R.string.developer,
        listOf("seed", "stage", "stage_frame", "player", "rank", "power", "continues", "invuln", "unlock", "enemy_dmg", "bomb",
            "screen_fx", "clrd", "replay_rate", "replay_save")),
)
