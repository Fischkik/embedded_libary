use crate::prelude::*;
use sim_core::ble::channel_mhz;
use sim_core::radio::{BLE_WIDTH_MHZ, ISM_MHZ, WIFI_OCCUPIED_MHZ, ZIGBEE_WIDTH_MHZ, free_ble_data_channels, wifi_mhz, zigbee_mhz};

pub struct Band {
    wifi: [bool; 14],
    zigbee: [bool; 16],
}

impl Band {
    fn wifi_on(&self) -> Vec<u8> {
        (1..=14u8).filter(|c| self.wifi[*c as usize - 1]).collect()
    }

    fn zigbee_on(&self) -> Vec<u8> {
        (11..=26u8).filter(|c| self.zigbee[*c as usize - 11]).collect()
    }
}

impl Default for Band {
    fn default() -> Self {
        let mut wifi = [false; 14];
        for c in [1, 6, 11] {
            wifi[c - 1] = true;
        }
        Self { wifi, zigbee: [false; 16] }
    }
}

impl Demo for Band {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "Who else is on the air", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Wi-Fi networks on channel:");
                for c in 1..=14u8 {
                    ui.toggle_value(&mut self.wifi[c as usize - 1], format!("{c}")).on_hover_text(format!("A Wi-Fi network on channel {c}, centred at {} MHz and about 20 MHz wide. Channel 14 is only allowed in Japan, for old equipment.", wifi_mhz(c)));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Zigbee / Thread networks on channel:");
                for c in 11..=26u8 {
                    ui.toggle_value(&mut self.zigbee[c as usize - 11], format!("{c}")).on_hover_text(format!("An IEEE 802.15.4 network (Zigbee, Thread) on channel {c}, centred at {} MHz and 2 MHz wide.", zigbee_mhz(c)));
                }
            });
        });
        let (wifi, zigbee) = (self.wifi_on(), self.zigbee_on());
        let free = free_ble_data_channels(&wifi, &zigbee);
        let free_count = free.iter().filter(|f| **f).count();

        ui.label(RichText::new("The 2.4 GHz band, 2400 to 2483.5 MHz").strong());
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(900.0), 220.0), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, pal.canvas);
        let (lo, hi) = (2398.0, 2490.0);
        let x = |mhz: f64| rect.left() + 10.0 + ((mhz - lo) / (hi - lo)) as f32 * (rect.width() - 20.0);
        let small = egui::FontId::proportional(11.0);
        // The licence-free band.
        painter.rect_filled(egui::Rect::from_x_y_ranges(x(ISM_MHZ.0)..=x(ISM_MHZ.1), rect.top() + 4.0..=rect.bottom() - 26.0), 0.0, pal.grid.gamma_multiply(0.35));
        for mhz in (2400..=2480).step_by(20) {
            painter.line_segment([pos2(x(mhz as f64), rect.bottom() - 26.0), pos2(x(mhz as f64), rect.bottom() - 20.0)], (1.0, pal.dim));
            painter.text(pos2(x(mhz as f64), rect.bottom() - 4.0), egui::Align2::CENTER_BOTTOM, format!("{mhz} MHz"), small.clone(), pal.dim);
        }
        // Wi-Fi: wide humps in the top half.
        for (k, &c) in wifi.iter().enumerate() {
            let f = wifi_mhz(c);
            let top = rect.top() + 14.0 + (k % 2) as f32 * 14.0;
            let r = egui::Rect::from_x_y_ranges(x(f - WIFI_OCCUPIED_MHZ / 2.0)..=x(f + WIFI_OCCUPIED_MHZ / 2.0), top..=rect.top() + 92.0);
            painter.rect_filled(r, 6.0, pal.trace[3].gamma_multiply(0.35));
            painter.rect_stroke(r, 6.0, (1.0, pal.trace[3]), egui::StrokeKind::Inside);
            painter.text(r.center_top() + vec2(0.0, 9.0), egui::Align2::CENTER_CENTER, format!("Wi-Fi {c}"), small.clone(), ui.visuals().strong_text_color());
        }
        // Zigbee / Thread: narrow bars.
        for &c in &zigbee {
            let f = zigbee_mhz(c);
            let r = egui::Rect::from_x_y_ranges(x(f - ZIGBEE_WIDTH_MHZ / 2.0)..=x(f + ZIGBEE_WIDTH_MHZ / 2.0), rect.top() + 60.0..=rect.top() + 92.0);
            painter.rect_filled(r, 1.0, pal.trace[4]);
        }
        // Bluetooth LE: 40 channels in the lower half.
        for i in 0..40u8 {
            let f = channel_mhz(i) as f64;
            let adv = i >= 37;
            let usable = adv || free[i as usize];
            let color = if adv { pal.trace[1] } else if usable { pal.good } else { pal.bad.gamma_multiply(0.5) };
            let r = egui::Rect::from_x_y_ranges(x(f - BLE_WIDTH_MHZ / 2.0 + 0.15)..=x(f + BLE_WIDTH_MHZ / 2.0 - 0.15), rect.top() + 110.0..=rect.top() + 160.0);
            painter.rect_filled(r, 1.0, color);
            if adv || i % 6 == 0 {
                painter.text(pos2(r.center().x, rect.top() + 172.0), egui::Align2::CENTER_CENTER, format!("{i}"), small.clone(), pal.text);
            }
        }
        widgets::key(
            ui,
            &[
                (pal.trace[3], "Wi-Fi network (the part it really occupies, 16 MHz)"),
                (pal.trace[4], "Zigbee / Thread network"),
                (pal.trace[1], "Bluetooth LE advertising channels 37, 38, 39"),
                (pal.good, "Bluetooth LE data channel, free"),
                (pal.bad, "data channel overlapped: a connection leaves it out"),
            ],
        );

        let map: Vec<String> = free.iter().enumerate().filter(|(_, f)| **f).map(|(i, _)| i.to_string()).collect();
        let adv_hit: Vec<u8> = (37..40u8)
            .filter(|&i| {
                let f = channel_mhz(i) as f64;
                wifi.iter().any(|&w| sim_core::radio::overlaps(f, BLE_WIDTH_MHZ, wifi_mhz(w), WIFI_OCCUPIED_MHZ)) || zigbee.iter().any(|&z| sim_core::radio::overlaps(f, BLE_WIDTH_MHZ, zigbee_mhz(z), ZIGBEE_WIDTH_MHZ))
            })
            .collect();
        widgets::formula(
            ui,
            format!(
                "Bluetooth LE: 40 channels, 2402 + 2k MHz for k = 0 .. 39 (in order of frequency)\n\
                 Wi-Fi channel n: 2407 + 5n MHz        Zigbee / Thread channel k: 2405 + 5(k - 11) MHz\n\
                 data channels still free: {free_count} of 37{}",
                if map.is_empty() { String::new() } else { format!(" ({})", map.join(" ")) },
            ),
        );
        widgets::status(
            ui,
            free_count >= 2 && adv_hit.is_empty(),
            if free_count < 2 {
                "fewer than two data channels are free: a Bluetooth LE connection needs at least two to hop between".to_owned()
            } else if !adv_hit.is_empty() {
                format!("the advertising channel(s) {adv_hit:?} overlap a network: finding the device gets harder, because advertising cannot avoid them")
            } else {
                format!("all three advertising channels are clear, and a connection can hop over {free_count} free data channels")
            },
        );
        if ui.button("See a connection hop: open \"A BLE connection: hopping\"").on_hover_text("The demo in which a connection moves from channel to channel.").clicked() {
            cx.open = Some(("ble-connection", Vec::new()));
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "none" => self.wifi = [false; 14],
            "europe" => {
                self.wifi = [false; 14];
                for c in [1, 5, 9, 13] {
                    self.wifi[c - 1] = true;
                }
            }
            "zigbee" => {
                self.wifi = [false; 14];
                self.zigbee[15] = true;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "band-2g4",
        title: "The 2.4 GHz band and its neighbours",
        blurb: "Bluetooth shares a narrow strip of frequencies with Wi-Fi, Zigbee and microwave ovens. See where everyone sits, and how Bluetooth keeps out of the way.",
        path: &["Wireless", "Radio basics"],
        make: || Box::new(Band::default()),
        explain: Explanation {
            what: "The strip is the 2.4 GHz band, the range of frequencies from 2400 to 2483.5 MHz. The wide orange blocks at the \
                top are Wi-Fi networks; narrow purple bars are Zigbee or Thread networks. Below are Bluetooth LE's 40 \
                [[channel]]s, each 2 MHz wide.\n\n\
                Green channels are free. Red ones overlap a network and are left out by a Bluetooth connection. The three \
                yellow ones are the advertising channels.",
            why: "In most of the radio spectrum, sending needs a licence. A few bands are licence-free, and 2.4 GHz is the one \
                that is free nearly everywhere in the world. That is why Wi-Fi, Bluetooth, Zigbee, Thread, cordless gadgets \
                and microwave ovens (which leak a little) all crowd into the same 83.5 MHz. The price of free is sharing: \
                nobody has the band to themselves.\n\n\
                Each system divides the band its own way. Wi-Fi uses wide channels: each network spreads over about 20 MHz, \
                so although Wi-Fi numbers 13 channels, only three fit side by side without overlapping - 1, 6 and 11 is the \
                usual choice. Zigbee and Thread use 16 narrow channels. Bluetooth LE uses 40 narrow ones.\n\n\
                Bluetooth's designers placed its three advertising channels deliberately. Advertising is how devices find each \
                other, so it must not depend on luck: 37, 38 and 39 sit at 2402, 2426 and 2480 MHz, in the gaps below, \
                between and above the usual Wi-Fi channels 1, 6 and 11. That is also why the numbering looks odd: the \
                advertising channels were numbered after the data channels, not in order of frequency.\n\n\
                Once two devices are connected, they [[hop]] over the 37 data channels. Both sides keep a channel map, a \
                list of which channels to use, and channels found to be busy can be removed from it. A connection then simply \
                hops around a Wi-Fi network instead of through it. That is what the red channels show.",
            formulas: &[
                ("Bluetooth LE: 40 channels at 2402, 2404, ... 2480 MHz (2 MHz apart)", "Advertising 37 = 2402 MHz, 38 = 2426 MHz, 39 = 2480 MHz; data channel 0 = 2404 MHz."),
                ("Wi-Fi channel n at 2407 + 5n MHz, about 20 MHz wide", "Channel 1 = 2412 MHz, 6 = 2437 MHz, 11 = 2462 MHz: far enough apart not to overlap."),
            ],
            try_this: &[
                Experiment { action: "Switch all Wi-Fi off.", watch: "all 37 data channels turning green. This is the quiet case the specification hopes for.", preset: Some("none") },
                Experiment {
                    action: "Use the European pattern of four networks: Wi-Fi channels 1, 5, 9 and 13.",
                    watch: "only 3 of the 37 data channels staying green, and two of the three advertising channels now overlapped: 38 by Wi-Fi channel 5 and 39 by channel 13. Bluetooth's careful placement assumes 1, 6 and 11.",
                    preset: Some("europe"),
                },
                Experiment {
                    action: "Remove the Wi-Fi and run one Zigbee network on channel 26.",
                    watch: "a single narrow bar at 2480 MHz - exactly on top of advertising channel 39. Narrow neighbours can collide too.",
                    preset: Some("zigbee"),
                },
            ],
            leaves_out: "Overlap is treated as all or nothing; in reality a Wi-Fi network's energy falls off gradually towards its \
                edges, and whether it disturbs Bluetooth depends on how busy and how strong it is. Wi-Fi is drawn at its \
                16 MHz occupied width; older 802.11b networks are 22 MHz wide, and newer ones can bond two channels into one \
                of 40 MHz. Which Wi-Fi channels are allowed differs by country. Bluetooth's adaptive frequency hopping needs \
                the devices to notice bad channels first; how they measure that is up to each implementation.",
            refs: &[
                "Bluetooth Core Specification 5.4, Vol 6 Part A section 2: 40 RF channels with centre frequencies 2 MHz apart from 2402 to 2480 MHz; advertising channels 37, 38, 39 at 2402, 2426, 2480 MHz.",
                "Wi-Fi 2.4 GHz channels (2412 MHz for channel 1, 5 MHz apart, 2484 MHz for channel 14; OFDM occupies 16.25 MHz): Wikipedia, \"List of WLAN channels\".",
                "IEEE 802.15.4 2.4 GHz channels 11-26 at 2405 + 5(k - 11) MHz, 2 MHz wide: Nordic Semiconductor product specifications; Wikipedia, \"Zigbee\".",
            ],
        },
    }
}
