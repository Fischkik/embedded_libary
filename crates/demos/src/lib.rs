//! The demo library. One module per demo; each exposes `entry()`.
//! To add a demo: write the module, list it in [`registry`], and add it to a path.

use lab_ui::{LearningPath, Planned, Registry};

pub(crate) mod prelude {
    pub use egui::{Color32, Pos2, RichText, Ui, pos2, vec2};
    pub use lab_ui::widgets::Clock;
    pub use lab_ui::{Demo, DemoCx, DemoEntry, Experiment, Explanation, bits, heat, plots, scene, theme, widgets};
    pub use sim_core::units::{C, si};
}

pub mod wireless {
    pub mod ble_adv_packet;
    pub mod ble_advertising;
    pub mod ble_connection;
    pub mod mesh_flooding;
    pub mod mesh_pdu;
    pub mod modulation;
    pub mod spectrum;
    pub mod wave_basics;
    pub mod wave_field;
}

pub mod radar {
    pub mod doppler;
    pub mod fmcw;
    pub mod range_doppler;
}

pub mod mcu {
    pub mod gpio;
    pub mod interrupts;
    pub mod pwm;
}

pub fn registry() -> Registry {
    Registry {
        demos: vec![
            wireless::wave_basics::entry(),
            wireless::wave_field::entry(),
            wireless::spectrum::entry(),
            wireless::modulation::entry(),
            wireless::ble_adv_packet::entry(),
            wireless::ble_advertising::entry(),
            wireless::ble_connection::entry(),
            wireless::mesh_flooding::entry(),
            wireless::mesh_pdu::entry(),
            radar::doppler::entry(),
            radar::fmcw::entry(),
            radar::range_doppler::entry(),
            mcu::gpio::entry(),
            mcu::pwm::entry(),
            mcu::interrupts::entry(),
        ],
        paths: vec![
            LearningPath {
                id: "wave-to-mesh",
                title: "From radio wave to mesh message",
                blurb: "Start with a bare wave and climb, layer by layer, to a Bluetooth Mesh network.",
                steps: &["wave-basics", "wave-field", "spectrum", "modulation", "ble-adv-packet", "ble-advertising", "ble-connection", "mesh-flooding", "mesh-pdu"],
            },
            LearningPath {
                id: "radar",
                title: "Radar: seeing with radio",
                blurb: "How a sensor detects movement, measures distance, and tells objects apart.",
                steps: &["wave-basics", "wave-field", "spectrum", "radar-doppler", "radar-fmcw", "radar-range-doppler"],
            },
            LearningPath {
                id: "mcu",
                title: "Inside the microcontroller",
                blurb: "The hardware your firmware talks to: pins, timers and interrupts.",
                steps: &["mcu-gpio", "mcu-pwm", "mcu-interrupts"],
            },
        ],
        planned: planned(),
    }
}

/// The roadmap: demos that are planned but not built yet. They appear greyed out in
/// the library so the whole territory is visible from the start.
fn planned() -> Vec<Planned> {
    const ITEMS: &[(&[&str], &[&str])] = &[
        (&["Wireless", "Radio basics"], &["Antennas and radiation patterns", "Link budget: power, distance, sensitivity", "The 2.4 GHz band and its neighbours"]),
        (&["Wireless", "Bluetooth LE"], &["GATT: services and characteristics", "Pairing and encryption", "Power: how long does the battery last?"]),
        (&["Wireless", "Bluetooth Mesh"], &["Addresses, publish and subscribe", "Segmented messages", "Friends and low power nodes", "Provisioning a new device", "Replay protection and the IV index"]),
        (&["Radar"], &["Angle of arrival with two antennas", "Presence detection: breathing and micro-motion", "Detection thresholds (CFAR)", "Radar versus PIR sensors"]),
        (&["Inside the microcontroller"], &["ADC: measuring a voltage", "RTOS: tasks and scheduling", "Priority inversion", "Clocks and low-power modes", "DMA: moving data without the CPU", "Memory map, stack and heap", "Boot, bootloader and firmware update", "Watchdog"]),
        (&["Wired buses"], &["UART: bytes on a wire", "SPI", "I2C", "CAN", "RS-485", "DALI lighting bus", "USB basics"]),
        (&["Signals and analog"], &["Sampling and aliasing", "Digital filters", "Noise and averaging", "PLL: making frequencies", "Op-amp circuits", "PIR sensor front end", "Switching power supplies"]),
        (&["Firmware craft"], &["State machines", "Ring buffers", "Fixed-point arithmetic", "Checksums and CRCs", "Debugging with a logic analyzer"]),
    ];
    ITEMS.iter().flat_map(|(path, titles)| titles.iter().map(|t| Planned { title: t, path })).collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn library_has_no_content_problems() {
        let problems = super::registry().problems();
        assert!(problems.is_empty(), "\n{}", problems.join("\n"));
    }
}
