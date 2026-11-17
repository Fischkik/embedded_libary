use crate::prelude::*;
use sim_core::zephyr::BtBuild;

const LAYERS: [&str; 4] = ["Application", "Host", "Controller", "Radio hardware"];

/// For each action: (button text, role line for prj.conf, demo to open, what each of
/// application / host / HCI / controller / radio does).
const ACTIONS: [(&str, &str, &str, [&str; 5]); 3] = [
    (
        "start advertising",
        "CONFIG_BT_PERIPHERAL=y",
        "ble-advertising",
        [
            "bt_enable(NULL), then bt_le_adv_start(...) with the device name to announce.",
            "Builds the advertising data and tells the controller what to send and how often.",
            "Three commands: LE Set Advertising Parameters, LE Set Advertising Data, LE Set Advertising Enable.",
            "The link layer sends the packet on channels 37, 38 and 39, and again every advertising interval, timed to the microsecond.",
            "Turns the bits into a 2.4 GHz radio signal.",
        ],
    ),
    (
        "send a value over a connection",
        "CONFIG_BT_PERIPHERAL=y",
        "ble-connection",
        [
            "bt_gatt_notify(conn, attr, &value, sizeof(value)): tell the connected phone that a value has changed.",
            "GATT and ATT wrap the value as a notification; L2CAP puts it on the connection's data channel.",
            "One data packet for this connection.",
            "The link layer sends it in the next connection event, on the next channel of the hopping sequence, and repeats it until it is acknowledged.",
            "Turns the bits into a 2.4 GHz radio signal.",
        ],
    ),
    (
        "publish a mesh message",
        "CONFIG_BT_MESH=y",
        "mesh-flooding",
        [
            "bt_mesh_model_publish(model): for example \"light on\" to a group of lamps.",
            "The mesh stack, which sits on top of the host, encrypts the message and hands it down as advertising data.",
            "The advertising commands again: to the controller, a mesh message is just another advertising packet.",
            "The link layer sends it as an advertising packet that nobody can connect to, on channels 37, 38 and 39.",
            "Turns the bits into a 2.4 GHz radio signal.",
        ],
    ),
];

pub struct Bluetooth {
    build: BtBuild,
    action: usize,
}

impl Default for Bluetooth {
    fn default() -> Self {
        Self { build: BtBuild::Combined, action: 0 }
    }
}

impl Demo for Bluetooth {
    fn ui(&mut self, ui: &mut Ui, cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "How the stack is split, and what the application does", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Build:");
                ui.selectable_value(&mut self.build, BtBuild::Combined, "everything in one chip").on_hover_text("Combined build: application, host and controller in one firmware on one Bluetooth chip.");
                ui.selectable_value(&mut self.build, BtBuild::HostOnly, "host here, controller in a second chip").on_hover_text("Host-only build: your microcontroller runs the application and the host; a separate Bluetooth chip runs the controller.");
                ui.selectable_value(&mut self.build, BtBuild::ControllerOnly, "controller only").on_hover_text("Controller-only build: this firmware turns a chip into a Bluetooth adapter for a host that runs somewhere else, for example on a PC.");
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("The application wants to:");
                for (i, a) in ACTIONS.iter().enumerate() {
                    ui.selectable_value(&mut self.action, i, a.0).on_hover_text("Follow this request down through the layers in the picture.");
                }
            });
        });
        let (build, action) = (self.build, ACTIONS[self.action]);
        let split = build != BtBuild::Combined;

        ui.label(RichText::new("The Bluetooth stack, top to bottom. Blue = part of the firmware you build").strong());
        let hci_text = if split { "HCI over a wire between two chips (UART, SPI or USB)" } else { "HCI: plain function calls inside the one chip" };
        let rows: [(Option<usize>, &str, &str); 5] = [(Some(0), LAYERS[0], action.3[0]), (Some(1), LAYERS[1], action.3[1]), (None, hci_text, action.3[2]), (Some(2), LAYERS[2], action.3[3]), (Some(3), LAYERS[3], action.3[4])];
        let chip = |layer: usize| match (build, layer <= 1) {
            (BtBuild::Combined, _) => "the Bluetooth chip",
            (BtBuild::HostOnly, true) => "chip 1: your microcontroller",
            (BtBuild::HostOnly, false) => "chip 2: a Bluetooth chip with its own firmware",
            (BtBuild::ControllerOnly, true) => "elsewhere: a PC or another processor",
            (BtBuild::ControllerOnly, false) => "the Bluetooth chip",
        };
        for (layer, name, does) in rows {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(230.0, if layer.is_some() { 46.0 } else { 26.0 }), egui::Sense::hover());
                let painter = ui.painter();
                match layer {
                    Some(l) => {
                        let mine = build.contains(l);
                        painter.rect_filled(rect, 4.0, if mine { pal.trace[0].gamma_multiply(0.45) } else { pal.canvas });
                        painter.rect_stroke(rect, 4.0, (1.5, if mine { pal.trace[0] } else { pal.dim }), egui::StrokeKind::Inside);
                        painter.text(rect.center() - vec2(0.0, 9.0), egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(15.0), ui.visuals().strong_text_color());
                        painter.text(rect.center() + vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, chip(l), egui::FontId::proportional(11.5), pal.text);
                    }
                    None => {
                        painter.rect_filled(rect.shrink2(vec2(0.0, 6.0)), 0.0, if split { pal.warn } else { pal.grid });
                        painter.text(rect.center(), egui::Align2::CENTER_CENTER, if split { "HCI: a cable" } else { "HCI" }, egui::FontId::proportional(12.0), pal.text);
                    }
                }
                ui.vertical(|ui| {
                    if layer.is_none() {
                        ui.label(RichText::new(name).small().color(pal.dim));
                    }
                    ui.label(does);
                });
            });
        }

        widgets::status(
            ui,
            build != BtBuild::ControllerOnly,
            match build {
                BtBuild::Combined => "one chip, one firmware: the usual choice for a small Bluetooth product".to_owned(),
                BtBuild::HostOnly => "two chips: your application gains a Bluetooth link without the radio's strict timing in its own processor".to_owned(),
                BtBuild::ControllerOnly => "this firmware contains no application of yours: the chip is a Bluetooth adapter, and the calls in the top two rows are made on the other side of the cable".to_owned(),
            },
        );
        widgets::formula(
            ui,
            format!(
                "# prj.conf for this build\n{}\n{}",
                build.prj_conf(),
                match build {
                    BtBuild::ControllerOnly => "# built from the sample hci_uart (or hci_usb, hci_spi)".to_owned(),
                    _ => format!("{}        # the part of Bluetooth this application uses", action.1),
                }
            ),
        );
        if ui.button(format!("Show me what goes over the air: open \"{}\"", action.2)).on_hover_text("Opens the demo from the path \"From radio wave to mesh message\" that shows this on the radio side.").clicked() {
            cx.open = Some((action.2, Vec::new()));
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "two-chips" => self.build = BtBuild::HostOnly,
            "adapter" => self.build = BtBuild::ControllerOnly,
            "mesh" => self.action = 2,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-bluetooth",
        title: "Bluetooth in Zephyr: host, controller and mesh",
        blurb: "One function call in your application, five layers down to the antenna - and three ways to split them between chips.",
        path: &["Zephyr"],
        make: || Box::new(Bluetooth::default()),
        explain: Explanation {
            what: "The picture is [[Zephyr]]'s Bluetooth stack as a pile of layers, the application on top and the radio at the \
                bottom. Blue boxes are part of the firmware you build; the small print in each box says which chip it runs \
                on. Beside each layer is what that layer does for the request chosen at the top.\n\n\
                The thin band in the middle is the [[HCI]], the one place where the pile can be cut in two.",
            why: "Bluetooth LE is specified in two halves with very different characters.\n\n\
                The controller is the half that deals with time. It implements the [[link layer]]: it decides, to the \
                microsecond, when the radio transmits and listens, on which channel, and it repeats packets that were not \
                acknowledged. Nothing may delay it, or connections break.\n\n\
                The host is the half that deals with meaning. It knows what advertising data says, what a connection is \
                for, which values a device offers and who may read them, and how two devices agree on keys. It is ordinary \
                software with no hard deadlines.\n\n\
                Between them the Bluetooth specification defines the host controller interface: a fixed set of commands, \
                events and data packets. Because that interface is standard, the two halves do not have to come from the \
                same maker, or even run on the same chip.\n\n\
                Zephyr contains both halves, and lets you build them three ways. Combined: one firmware in one Bluetooth \
                chip, with the two halves talking through function calls - the normal case for a small product. Host only: \
                your application and the host run on your own microcontroller, and a second chip with its own firmware is \
                the controller; the two talk HCI over a serial cable. Controller only: the chip becomes a Bluetooth adapter \
                for a host somewhere else, such as a PC.\n\n\
                Bluetooth Mesh is not a third half. It is a further layer on top of the host that uses ordinary advertising \
                packets as its means of transport, which is why the lower rows hardly change when you publish a mesh \
                message.",
            formulas: &[("application -> host -> HCI -> controller (link layer) -> radio", "Every Bluetooth call your application makes travels down this chain; everything received travels up it.")],
            try_this: &[
                Experiment {
                    action: "Choose \"host here, controller in a second chip\".",
                    watch: "the pile splitting at the HCI band, which becomes a cable. The rows of text do not change: the same commands now travel over a wire instead of through function calls.",
                    preset: Some("two-chips"),
                },
                Experiment {
                    action: "Choose \"controller only\".",
                    watch: "only the lower two boxes staying blue. Your firmware is now just the bridge; the application and host are someone else's, on the other end of the cable.",
                    preset: Some("adapter"),
                },
                Experiment {
                    action: "Back in one chip, let the application publish a mesh message.",
                    watch: "the controller's row: to it, the mesh message is one more advertising packet. Then use the button at the bottom to see the message spread through a mesh.",
                    preset: Some("mesh"),
                },
            ],
            leaves_out: "The host is drawn as one box; inside it are layers of its own (L2CAP, ATT, GATT, SMP and the GAP \
                procedures), which the packet and connection demos of the radio path cover from the air side. Each request is \
                reduced to its main steps; real calls take more parameters and report errors. A mesh node also has to be \
                provisioned before it can publish, and mesh can alternatively run over connections. Zephyr's own controller \
                supports a limited set of radio chips; other chips bring their maker's controller behind the same HCI.",
            refs: &[
                "Zephyr documentation, Bluetooth > Stack Architecture: Host, Controller, Radio Hardware; HCI over UART, SPI or USB; controller-only (hci_uart, CONFIG_BT_HCI_RAW), host-only and combined builds.",
                "Bluetooth Core Specification, Vol 4 Part E (HCI): LE Set Advertising Parameters, LE Set Advertising Data, LE Set Advertising Enable.",
                "Zephyr Bluetooth API: bt_enable, bt_le_adv_start, bt_gatt_notify, bt_mesh_model_publish.",
            ],
        },
    }
}
