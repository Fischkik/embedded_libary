//! Headless tests of the whole app: every demo is opened, run and rendered to a PNG
//! under `target/shots/` so its layout can be looked at without opening a window.

use egui_kittest::Harness;
use embedded_lab::{Saved, Shell};

fn harness(shell: Shell, size: [f32; 2]) -> Harness<'static, Shell> {
    Harness::builder()
        .with_size(egui::vec2(size[0], size[1]))
        .with_step_dt(1.0 / 30.0)
        .wgpu()
        .build_ui_state(|ui, shell: &mut Shell| shell.ui(ui), shell)
}

fn shot(h: &mut Harness<'static, Shell>, name: &str) {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shots");
    std::fs::create_dir_all(&dir).unwrap();
    h.render().expect("render").save(dir.join(format!("{name}.png"))).unwrap();
}

#[test]
fn every_demo_runs_and_renders() {
    let count = demos::registry().demos.len();
    for i in 0..count {
        let mut shell = Shell::new(demos::registry(), Saved::default());
        shell.select(i, None);
        let id = shell.current_id();
        let presets: Vec<&'static str> = shell.reg.demos[i].explain.try_this.iter().filter_map(|x| x.preset).collect();
        let mut h = harness(shell, [1500.0, 950.0]);
        h.run_steps(20);
        shot(&mut h, id);
        for p in presets {
            h.state_mut().apply_preset(p);
            h.run_steps(6);
        }
        shot(&mut h, &format!("{id}--last-preset"));
    }
}

#[test]
fn works_in_a_small_window_and_in_light_mode() {
    let count = demos::registry().demos.len();
    for i in 0..count {
        let mut shell = Shell::new(demos::registry(), Saved::default());
        shell.select(i, None);
        let mut h = harness(shell, [900.0, 600.0]);
        h.ctx.set_theme(egui::Theme::Light);
        h.run_steps(8);
        if i == 0 {
            shot(&mut h, "small-light");
        }
    }
}

use egui_kittest::kittest::Queryable;

fn open(id: &str) -> Harness<'static, Shell> {
    let reg = demos::registry();
    let index = reg.index_of(id).unwrap();
    let mut shell = Shell::new(reg, Saved::default());
    shell.select(index, None);
    harness(shell, [1500.0, 950.0])
}

#[test]
fn next_and_back_buttons_walk_the_path() {
    let mut h = open("wave-basics");
    h.run_steps(2);
    h.get_by_label("Next >").click();
    h.run_steps(3);
    assert_eq!(h.state().current_id(), "wave-field");
    h.get_by_label("< Back").click();
    h.run_steps(3);
    assert_eq!(h.state().current_id(), "wave-basics");
}

#[test]
fn a_damaged_bit_makes_the_receiver_reject_the_packet() {
    let mut h = open("ble-adv-packet");
    h.run_steps(3);
    assert!(h.query_by_label_contains("CRC matches").is_some());
    h.state_mut().apply_preset("damage");
    h.run_steps(3);
    assert!(h.query_by_label_contains("CRC does not match").is_some());
    assert!(h.query_by_label_contains("CRC matches").is_none());
}

#[test]
fn inspecting_a_mesh_transmission_opens_the_packet_demo() {
    let mut h = open("mesh-flooding");
    h.run_steps(3);
    h.get_all_by_label("Inspect packet").next().unwrap().click();
    h.run_steps(3);
    assert_eq!(h.state().current_id(), "mesh-pdu");
    // The receiver in the packet demo accepts the untampered packet.
    assert!(h.query_by_label_contains("NetMIC correct").is_some());
}

#[test]
fn navigation_follows_the_learning_path() {
    let mut shell = Shell::new(demos::registry(), Saved::default());
    shell.select(0, None);
    let first = shell.current_id();
    let mut h = harness(shell, [1500.0, 950.0]);
    h.run_steps(3);
    assert_eq!(h.state().current_id(), first);
    assert!(h.state().saved.visited.iter().any(|v| v == first));
}
