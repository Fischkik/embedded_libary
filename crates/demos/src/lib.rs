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
    pub mod antenna;
    pub mod band;
    pub mod ble_adv_packet;
    pub mod ble_advertising;
    pub mod ble_connection;
    pub mod ble_power;
    pub mod gatt;
    pub mod link_budget;
    pub mod mesh_addressing;
    pub mod mesh_flooding;
    pub mod mesh_friend;
    pub mod mesh_pdu;
    pub mod mesh_provisioning;
    pub mod mesh_replay;
    pub mod mesh_segmentation;
    pub mod modulation;
    pub mod pairing;
    pub mod spectrum;
    pub mod wave_basics;
    pub mod wave_field;
}

pub mod radar {
    pub mod angle;
    pub mod cfar;
    pub mod doppler;
    pub mod fmcw;
    pub mod presence;
    pub mod range_doppler;
    pub mod vs_pir;
}

pub mod mcu {
    pub mod adc;
    pub mod boot;
    pub mod clocks;
    pub mod dma;
    pub mod gpio;
    pub mod interrupts;
    pub mod low_power;
    pub mod memory;
    pub mod priority_inversion;
    pub mod pwm;
    pub mod rtos;
    pub mod watchdog;
}

pub mod zephyr {
    pub mod bluetooth;
    pub mod boot;
    pub mod devicetree;
    pub mod drivers;
    pub mod isr;
    pub mod kconfig;
    pub mod logging;
    pub mod msgq;
    pub mod pm;
    pub mod semaphore;
    pub mod threads;
    pub mod timers;
    pub mod workqueue;
}

pub mod firmware {
    pub mod crc;
    pub mod fixed_point;
    pub mod logic_analyzer;
    pub mod ring_buffer;
    pub mod state_machine;
}

pub mod bus {
    pub mod can;
    pub mod dali;
    pub mod i2c;
    pub mod rs485;
    pub mod spi;
    pub mod uart;
    pub mod usb;
}

pub mod signals {
    pub mod buck;
    pub mod filter;
    pub mod noise;
    pub mod opamp;
    pub mod pir;
    pub mod pll;
    pub mod sampling;
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
            wireless::antenna::entry(),
            wireless::link_budget::entry(),
            wireless::band::entry(),
            wireless::gatt::entry(),
            wireless::pairing::entry(),
            wireless::ble_power::entry(),
            wireless::mesh_addressing::entry(),
            wireless::mesh_segmentation::entry(),
            wireless::mesh_friend::entry(),
            wireless::mesh_provisioning::entry(),
            wireless::mesh_replay::entry(),
            radar::doppler::entry(),
            radar::fmcw::entry(),
            radar::range_doppler::entry(),
            radar::angle::entry(),
            radar::presence::entry(),
            radar::cfar::entry(),
            radar::vs_pir::entry(),
            mcu::gpio::entry(),
            mcu::pwm::entry(),
            mcu::adc::entry(),
            mcu::interrupts::entry(),
            mcu::rtos::entry(),
            mcu::low_power::entry(),
            mcu::priority_inversion::entry(),
            mcu::memory::entry(),
            mcu::watchdog::entry(),
            mcu::clocks::entry(),
            mcu::dma::entry(),
            mcu::boot::entry(),
            zephyr::threads::entry(),
            zephyr::devicetree::entry(),
            zephyr::kconfig::entry(),
            zephyr::semaphore::entry(),
            zephyr::msgq::entry(),
            zephyr::workqueue::entry(),
            zephyr::isr::entry(),
            zephyr::timers::entry(),
            zephyr::boot::entry(),
            zephyr::pm::entry(),
            zephyr::drivers::entry(),
            zephyr::logging::entry(),
            zephyr::bluetooth::entry(),
            firmware::state_machine::entry(),
            firmware::ring_buffer::entry(),
            firmware::fixed_point::entry(),
            firmware::crc::entry(),
            firmware::logic_analyzer::entry(),
            bus::uart::entry(),
            bus::spi::entry(),
            bus::i2c::entry(),
            bus::can::entry(),
            bus::rs485::entry(),
            bus::dali::entry(),
            bus::usb::entry(),
            signals::sampling::entry(),
            signals::noise::entry(),
            signals::filter::entry(),
            signals::pll::entry(),
            signals::opamp::entry(),
            signals::pir::entry(),
            signals::buck::entry(),
        ],
        paths: vec![
            LearningPath {
                id: "wave-to-mesh",
                title: "From radio wave to mesh message",
                blurb: "Start with a bare wave and climb, layer by layer, to a Bluetooth Mesh network.",
                steps: &["wave-basics", "wave-field", "antenna-pattern", "spectrum", "modulation", "link-budget", "band-2g4", "ble-adv-packet", "ble-advertising", "ble-connection", "mesh-flooding", "mesh-pdu", "ble-gatt", "ble-pairing", "ble-power", "mesh-addressing", "mesh-segmentation", "mesh-friend", "mesh-provisioning", "mesh-replay"],
            },
            LearningPath {
                id: "radar",
                title: "Radar: seeing with radio",
                blurb: "How a sensor detects movement, measures distance, and tells objects apart.",
                steps: &["wave-basics", "wave-field", "spectrum", "radar-doppler", "radar-fmcw", "radar-range-doppler", "radar-cfar", "radar-angle", "radar-presence", "radar-vs-pir"],
            },
            LearningPath {
                id: "mcu",
                title: "Inside the microcontroller",
                blurb: "The hardware your firmware talks to - pins, clocks, timers, converters, interrupts, memory - then sharing the processor, staying alive, saving power and updating in the field.",
                steps: &["mcu-gpio", "mcu-clocks", "mcu-pwm", "mcu-adc", "mcu-interrupts", "mcu-dma", "mcu-memory", "mcu-rtos", "mcu-priority-inversion", "mcu-watchdog", "mcu-low-power", "mcu-boot"],
            },
            LearningPath {
                id: "zephyr",
                title: "Zephyr: a real-time operating system in practice",
                blurb: "From interrupts and the idea of a scheduler to a real RTOS: how Zephyr is told about your board, configured, and how it runs your threads.",
                steps: &["mcu-interrupts", "mcu-rtos", "zephyr-devicetree", "zephyr-kconfig", "zephyr-boot", "zephyr-drivers", "zephyr-threads", "zephyr-interrupts", "zephyr-semaphore", "zephyr-msgq", "zephyr-workqueue", "zephyr-timers", "zephyr-logging", "zephyr-pm", "zephyr-bluetooth"],
            },
            LearningPath {
                id: "firmware",
                title: "Firmware craft",
                blurb: "Techniques that turn up in every firmware project, whatever the chip.",
                steps: &["firmware-state-machine", "firmware-ring-buffer", "firmware-fixed-point", "firmware-crc", "firmware-logic-analyzer"],
            },
            LearningPath {
                id: "buses",
                title: "Wired buses: how chips and devices talk",
                blurb: "From one wire carrying one byte to shared buses with addresses, priorities and long cables.",
                steps: &["bus-uart", "bus-spi", "bus-i2c", "bus-rs485", "bus-can", "bus-dali", "bus-usb"],
            },
            LearningPath {
                id: "signals",
                title: "Signals and analog",
                blurb: "Where the real world meets the microcontroller: sampling, noise, filters, and the circuits around a sensor.",
                steps: &["signals-sampling", "signals-noise", "signals-filter", "signals-opamp", "signals-pir", "signals-pll", "signals-buck"],
            },
        ],
        planned: planned(),
    }
}

/// The roadmap: demos that are planned but not built yet. They appear greyed out in
/// the library so the whole territory is visible from the start.
fn planned() -> Vec<Planned> {
    // Every planned topic is built. Add new ones here as (path, titles).
    const ITEMS: &[(&[&str], &[&str])] = &[];
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
