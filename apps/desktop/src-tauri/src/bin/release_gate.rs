#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    jyroxa_desktop_lib::release_gate::run();
}
