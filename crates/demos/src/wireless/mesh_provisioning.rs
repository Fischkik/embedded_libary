use crate::prelude::*;
use sim_core::crypto::ccm_encrypt;
use sim_core::mesh::{Key, auth_value, check_confirmation, mitm_accepted, prov_side, provisioning_data, session_keys};
use sim_core::units::hex;

#[derive(Clone, Copy, PartialEq)]
enum Oob {
    None,
    Number,
}

pub struct MeshProvisioning {
    oob: Oob,
    number: u32,
    attacker: bool,
    address: u16,
}

impl Default for MeshProvisioning {
    fn default() -> Self {
        Self { oob: Oob::Number, number: 482_915, attacker: false, address: 0x0B0C }
    }
}

/// Stand-ins for what the elliptic-curve exchange would produce; it is not computed here.
const SECRET_HONEST: [u8; 32] = [0x5A; 32];
const SECRET_WITH_ATTACKER: [u8; 32] = [0x33; 32];
const NET_KEY: Key = [0x7D, 0xD7, 0x36, 0x4C, 0xD8, 0x42, 0xAD, 0x18, 0xC1, 0x7C, 0x2B, 0x82, 0x0C, 0x84, 0xC3, 0xD6];

impl Demo for MeshProvisioning {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A phone adds a new lamp to the mesh", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("How the lamp proves who it is:");
                ui.selectable_value(&mut self.oob, Oob::None, "no way (No OOB)").on_hover_text("The lamp has no display, no button, no label. Nothing is checked outside the radio.");
                ui.selectable_value(&mut self.oob, Oob::Number, "it blinks a number (Output OOB)").on_hover_text("The lamp shows a number - for example by blinking - and the user types it into the phone. OOB means 'out of band': through another channel than the radio.");
                widgets::wrap(ui, "number", |ui| ui.add_enabled(self.oob == Oob::Number, egui::Slider::new(&mut self.number, 0..=999_999).text("the number").custom_formatter(|v, _| format!("{:06}", v as u32))).on_hover_text("The number the lamp shows and the user types in."));
            });
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.attacker, "an attacker sits in the middle").on_hover_text("A device nearby pretends to be the lamp towards the phone and the phone towards the lamp, and relays everything in between.");
                widgets::wrap(ui, "addr", |ui| ui.add(egui::Slider::new(&mut self.address, 0x0001..=0x7FFF).text("address given to the lamp").hexadecimal(4, false, true)).on_hover_text("The unicast address the phone assigns to the new lamp."));
            });
        });
        let auth = auth_value(if self.oob == Oob::Number { Some(self.number) } else { None });
        let inputs = vec![0x11u8; 145];
        let (rp, rd): (Key, Key) = ([0xA1; 16], [0xD7; 16]);
        let secret = if self.attacker { SECRET_WITH_ATTACKER } else { SECRET_HONEST };
        let phone = prov_side(&secret, &inputs, &rp, &auth);
        let lamp = prov_side(&secret, &inputs, &rd, &auth);
        let accepted = if self.attacker {
            // The attacker does not know the number and guesses 000000 (or knows it is zero).
            mitm_accepted(&secret, &inputs, &auth, &auth_value(if self.oob == Oob::Number { Some(0) } else { None }), &[0x99; 16], &rp)
        } else {
            check_confirmation(&phone, &lamp.own_confirmation, &rd, &auth)
        };

        ui.label(RichText::new("The five phases of provisioning").strong());
        let step = |ui: &mut Ui, n: &str, title: &str, what: &str, value: Option<String>, ok: Option<bool>| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(format!("{n}. {title}")).strong());
                    if let Some(ok) = ok {
                        ui.label(RichText::new(if ok { "passes" } else { "FAILS" }).strong().color(if ok { pal.good } else { pal.bad }));
                    }
                });
                ui.label(what);
                if let Some(v) = value {
                    ui.add(egui::Label::new(RichText::new(v).monospace().color(pal.trace[0])).extend());
                }
            });
        };
        step(ui, "1", "Beacon", "The new lamp sends \"unprovisioned device\" beacons with its 16-byte device UUID. The phone lists it.", None, None);
        step(ui, "2", "Invitation", "The phone invites it; the lamp answers with its capabilities - including how it can show or accept a number.", None, None);
        step(
            ui,
            "3",
            "Public keys",
            if self.attacker {
                "Each side sends a public key. The attacker intercepts both and sends its own instead, so it shares one secret with the phone and another with the lamp."
            } else {
                "Each side sends a public key. From its own private key and the other's public key, each computes the same shared secret (elliptic-curve Diffie-Hellman). An eavesdropper sees both public keys but cannot compute the secret."
            },
            Some(format!("shared secret = {}...\n(a stand-in: the elliptic-curve step is not computed)", hex(&secret[..8]))),
            None,
        );
        let auth_text = if self.oob == Oob::Number { format!("AuthValue = the number {:06} = {}", self.number, hex(&auth)) } else { format!("AuthValue = zero = {}", hex(&auth)) };
        step(
            ui,
            "4",
            "Authentication",
            "Both commit to a random number by sending Confirmation = AES-CMAC(ConfirmationKey, Random || AuthValue), then reveal the random numbers and check the other's confirmation. Only someone who knows the AuthValue can produce a confirmation that checks out.",
            Some(format!(
                "{auth_text}\nConfirmationKey = k1(secret, s1(ConfirmationInputs), \"prck\")\n                = {}\nphone's Confirmation = {}{}",
                hex(&phone.confirmation_key),
                hex(&phone.own_confirmation),
                if self.attacker { "\nthe attacker must answer without knowing the AuthValue" } else { "" },
            )),
            Some(accepted),
        );
        let keys = session_keys(&secret, &phone.confirmation_salt, &rp, &rd);
        let data = provisioning_data(&NET_KEY, 0x0000, 0x00, 0x1234_5678, self.address);
        let enc = ccm_encrypt(&keys.session_key, &keys.session_nonce, &[], &data, 8);
        step(
            ui,
            "5",
            "Distribution",
            if accepted {
                "The phone sends the network key, the IV index and the lamp's new address, encrypted with a session key derived from the shared secret. Both also derive a device key that only these two know."
            } else {
                "The phone stops: authentication failed, so no network key is sent."
            },
            accepted.then(|| format!("provisioning data, 25 octets:\n{}\nencrypted with SessionKey, plus an 8-octet MIC:\n{}\nDeviceKey = {}", rows(&data), rows(&enc), hex(&keys.device_key))),
            None,
        );

        let verdict = match (self.attacker, accepted) {
            (false, true) => (true, "the lamp has joined the network".to_owned()),
            (true, true) => (false, "the attacker passed and receives the network key: it can read and send everything".to_owned()),
            (true, false) => (true, "attack caught: without the lamp's number, no valid confirmation".to_owned()),
            (false, false) => (false, "authentication failed".to_owned()),
        };
        widgets::status(ui, verdict.0, verdict.1);
        widgets::formula(
            ui,
            "Confirmation = AES-CMAC(ConfirmationKey, Random || AuthValue)\n\
             SessionKey, SessionNonce, DeviceKey = k1(secret, ProvisioningSalt, \"prsk\" / \"prsn\" / \"prdk\")\n\
             chance of guessing a 6-digit number in one try = 1 in 1 000 000",
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "mitm-none" => (self.oob, self.attacker) = (Oob::None, true),
            "mitm-number" => self.attacker = true,
            "honest-none" => self.oob = Oob::None,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "mesh-provisioning",
        title: "Provisioning a new device",
        blurb: "How a brand-new lamp gets the network's secret key over the air - and why it should blink a number while doing so.",
        path: &["Wireless", "Bluetooth", "Bluetooth Mesh"],
        make: || Box::new(MeshProvisioning::default()),
        explain: Explanation {
            what: "The five boxes are the phases of [[provisioning]]: a phone (the provisioner) adding a new lamp to a Bluetooth Mesh \
                network. The values in phases 4 and 5 are computed with the specification's formulas. At the top you choose \
                how the lamp proves its identity, and whether an attacker is sitting in between.",
            why: "A new lamp out of the box knows nothing: no network key, no address. Everything it will ever know about the \
                network has to reach it over the air, and the air can be overheard. Provisioning solves this in two steps.\n\n\
                First, the two devices create a shared secret that an eavesdropper cannot learn, even though everything they \
                send is public. They use elliptic-curve Diffie-Hellman: each has a private number and sends a public value \
                computed from it; each combines its own private number with the other's public value, and the mathematics \
                guarantees that both arrive at the same result - while someone who saw only the two public values cannot. \
                From this secret both derive a session key, and the phone sends the network key, encrypted with it.\n\n\
                But Diffie-Hellman alone has a gap. An attacker in the middle can run it twice - once with the phone, \
                pretending to be the lamp, and once with the lamp, pretending to be the phone - and relay everything. Each \
                side then has a perfectly good secret, shared with the attacker.\n\n\
                Authentication closes the gap. The lamp shows a number, by blinking or on a display, and the user types it \
                into the phone. Both sides must now prove they know this number, the AuthValue, without sending it: each \
                first sends a confirmation - a cryptographic checksum over a random number and the AuthValue - and only then \
                reveals the random number. The attacker never saw the lamp's blinking, so it cannot produce a confirmation \
                that checks out, and it has to commit before it learns anything that would help it guess.\n\n\
                With No OOB, the AuthValue is zero. Everything still looks secure and nobody can eavesdrop - but an active \
                attacker in the middle passes authentication with ease.",
            formulas: &[
                ("Confirmation = AES-CMAC(ConfirmationKey, Random || AuthValue)", "The ConfirmationKey depends on the shared secret, so the attacker computes it correctly for its half - but the AuthValue it must guess."),
                ("chance an attacker guesses a 6-digit number = 1 / 1 000 000", "And a failed guess ends the provisioning, so it gets only one try per attempt."),
            ],
            try_this: &[
                Experiment {
                    action: "Put an attacker in the middle of a lamp with No OOB.",
                    watch: "phase 4 passing anyway, and phase 5 handing the network key to the attacker. Nothing in the radio exchange can tell the difference.",
                    preset: Some("mitm-none"),
                },
                Experiment {
                    action: "Keep the attacker, but let the lamp blink a number.",
                    watch: "phase 4 failing and the network key never being sent: the attacker guessed 000000, and the number was 482915.",
                    preset: Some("mitm-number"),
                },
                Experiment {
                    action: "Without an attacker, change the address given to the lamp.",
                    watch: "the last two octets of the provisioning data changing, and the whole encrypted block changing with them.",
                    preset: None,
                },
            ],
            leaves_out: "The elliptic-curve exchange is not computed: the shared secret is a fixed stand-in, labelled as such. All values \
                after it are computed with the specification's formulas. With the specification's own sample secret and inputs, the same \
                code reproduces every published value (see the second reference). The \
                ConfirmationInputs (the five invitation, capability, start and public-key messages) are dummy bytes. Other \
                ways to authenticate exist: a number typed into the device, or a fixed secret printed on its label. Mesh \
                Protocol 1.1 adds a second, stronger algorithm based on HMAC-SHA-256 with longer random values.",
            refs: &[
                "Bluetooth Mesh Protocol 1.1, section 5 (provisioning: beaconing, invitation, exchanging public keys, authentication, distribution of provisioning data).",
                "Checked bit for bit against the specification's provisioning sample data (Mesh Protocol 1.1, 8.7.8-8.7.12): ConfirmationSalt, ConfirmationKey, both Confirmations, ProvisioningSalt, SessionKey, SessionNonce, DeviceKey, and the encrypted provisioning data with its MIC.",
                "Formulas as implemented in Zephyr, subsys/bluetooth/mesh/crypto.c: ConfirmationSalt = s1(ConfirmationInputs, 145 octets); ConfirmationKey = k1(ECDHSecret, ConfirmationSalt, \"prck\"); Confirmation = AES-CMAC(ConfirmationKey, Random || AuthValue); ProvisioningSalt = s1(ConfirmationSalt || RandomProvisioner || RandomDevice); SessionKey / SessionNonce (bytes 3-15) / DeviceKey from k1 with \"prsk\" / \"prsn\" / \"prdk\"; 25 octets of provisioning data, 8-octet MIC.",
            ],
        },
    }
}

/// Hex bytes, 16 to a line, so that long values do not make the demo wider than its area.
fn rows(bytes: &[u8]) -> String {
    bytes.chunks(16).map(hex).collect::<Vec<_>>().join("\n")
}
