mod input;

use macroquad::prelude::*;

const SCREEN_W: f32 = 640.0;
const SCREEN_H: f32 = 480.0;
const FRAME: f64 = 1.0 / 60.0;

fn conf() -> Conf {
    Conf {
        window_title: "th08".into(),
        window_width: SCREEN_W as i32,
        window_height: SCREEN_H as i32,
        window_resizable: false,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let mut input = input::Input::default();
    let mut acc = 0.0;
    let mut last = get_time();
    let mut frame: u64 = 0;
    loop {
        let now = get_time();
        acc += now - last;
        last = now;
        let mut steps = 0;
        while acc >= FRAME && steps < 4 {
            input.poll();
            frame += 1;
            acc -= FRAME;
            steps += 1;
        }
        if acc > FRAME * 4.0 {
            acc = 0.0;
        }
        clear_background(BLACK);
        draw_text(&format!("frame {frame}"), 10.0, 20.0, 20.0, WHITE);
        next_frame().await;
    }
}
