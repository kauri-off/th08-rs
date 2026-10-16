mod archive;
mod bgm;
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
    let dir = std::env::args().nth(1).or_else(|| std::env::var("TH08_DIR").ok()).expect("usage: th08 <game dir>");
    let dir = std::path::PathBuf::from(dir);
    let ar = archive::Archive::open(&dir.join("th08.dat")).expect("th08.dat");
    let mut bgm = bgm::Bgm::new(&dir.join("thbgm.dat"), &ar.read("thbgm.fmt").unwrap());
    bgm.play("th08_01.wav").unwrap();
    let title = Texture2D::from_file_with_format(&ar.read("title00.png").unwrap(), None);
    let mut input = input::Input::default();
    let mut acc = 0.0;
    let mut last = get_time();
    let mut frame: u64 = 0;
    let shot: Option<u64> = std::env::var("TH08_SHOT").ok().and_then(|v| v.parse().ok());
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
        draw_texture(&title, 0.0, 0.0, WHITE);
        draw_text(&format!("frame {frame}"), 10.0, 20.0, 20.0, WHITE);
        if shot == Some(frame) {
            get_screen_data().export_png("/tmp/th08_shot.png");
            break;
        }
        next_frame().await;
    }
}
