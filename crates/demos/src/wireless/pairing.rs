use crate::prelude::*;
use sim_core::rng::Rng;
use sim_core::smp::{Block, Pairing as Recorded, PairingContext, c1, crack, pair, s1, tk_from_passkey};
use sim_core::units::hex;

/// Passkeys tried per frame while the eavesdropper is guessing.
const PER_FRAME: u32 = 15_000;

pub struct Pairing {
    just_works: bool,
    passkey: u32,
    mrand: Block,
    srand: Block,
    cracking: bool,
    next_guess: u32,
    found: Option<u32>,
    /// The settings the search belongs to; a change restarts it.
    searched_for: (bool, u32),
}

impl Default for Pairing {
    fn default() -> Self {
        let mut rng = Rng::new(2024);
        let mut random = || -> Block { std::array::from_fn(|_| rng.next_u64() as u8) };
        let (mrand, srand) = (random(), random());
        Self { just_works: false, passkey: 123_456, mrand, srand, cracking: false, next_guess: 0, found: None, searched_for: (false, 123_456) }
    }
}

impl Pairing {
    fn tk_value(&self) -> u32 {
        if self.just_works { 0 } else { self.passkey }
    }
}

impl Demo for Pairing {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "A phone pairs with a sensor (LE legacy pairing)", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.just_works, false, "Passkey Entry").on_hover_text("The sensor shows a 6-digit number, the user types it into the phone.");
                ui.selectable_value(&mut self.just_works, true, "Just Works").on_hover_text("No number at all: the user just taps \"pair\". Used by devices without a display or keys.");
                widgets::wrap(ui, "passkey", |ui| {
                    ui.add_enabled(!self.just_works, egui::Slider::new(&mut self.passkey, 0..=999_999).text("passkey").custom_formatter(|v, _| format!("{:06}", v as u32))).on_hover_text("The 6-digit number shown on the sensor and typed into the phone. It never goes over the air.")
                });
            });
        });
        if self.searched_for != (self.just_works, self.passkey) {
            (self.cracking, self.next_guess, self.found, self.searched_for) = (false, 0, None, (self.just_works, self.passkey));
        }
        let ctx = PairingContext::spec_example();
        let p: Recorded = pair(ctx, self.tk_value(), self.mrand, self.srand);
        let tk = tk_from_passkey(self.tk_value());

        ui.label(RichText::new("The exchange, step by step").strong());
        let step = |ui: &mut Ui, n: &str, who: &str, what: &str, value: String, on_air: bool| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(n).strong());
                    ui.label(RichText::new(who).color(pal.dim));
                    ui.label(what);
                    ui.label(RichText::new(if on_air { "sent over the air" } else { "never sent" }).small().color(if on_air { pal.warn } else { pal.good }));
                });
                ui.add(egui::Label::new(RichText::new(value).monospace().color(pal.trace[0])).extend());
            });
        };
        step(ui, "1", "both", "exchange Pairing Request and Pairing Response (what each can do):", format!("{}   /   {}", hex(&ctx.preq), hex(&ctx.pres)), true);
        step(ui, "2", "both", if self.just_works { "set the temporary key TK to zero (Just Works):" } else { "put the passkey into the temporary key TK:" }, hex(&tk), false);
        step(ui, "3", "phone", "picks a random number Mrand and sends Mconfirm = c1(TK, Mrand, ...):", format!("Mconfirm = {}", hex(&p.mconfirm)), true);
        step(ui, "4", "sensor", "picks Srand and sends Sconfirm = c1(TK, Srand, ...):", format!("Sconfirm = {}", hex(&p.sconfirm)), true);
        step(ui, "5", "both", "reveal the random numbers; each checks the other's confirm value:", format!("Mrand = {}\nSrand = {}", hex(&p.mrand), hex(&p.srand)), true);
        step(ui, "6", "both", "compute the short-term key STK = s1(TK, Srand, Mrand) and start encrypting with it:", format!("STK = {}", hex(&p.stk)), false);
        let check = c1(&tk, &p.srand, &ctx) == p.sconfirm && c1(&tk, &p.mrand, &ctx) == p.mconfirm;
        widgets::status(ui, check, if check { "both confirm values check out: the two devices used the same passkey" } else { "a confirm value does not fit" });

        widgets::group(ui, "An eavesdropper recorded every packet marked \"sent over the air\"", |ui| {
            ui.horizontal_wrapped(|ui| {
                let label = if self.cracking { "Stop guessing" } else if self.found.is_some() { "Guess again" } else { "Start guessing passkeys" };
                if ui.button(label).on_hover_text("Try passkey 000000, 000001, ... and compute c1 for each, until one reproduces the recorded Mconfirm.").clicked() {
                    if self.found.is_some() {
                        (self.next_guess, self.found) = (0, None);
                    }
                    self.cracking = !self.cracking;
                }
                ui.add(egui::ProgressBar::new(self.next_guess as f32 / 1_000_000.0).desired_width(260.0).text(format!("{} of 1 000 000 passkeys tried", self.next_guess)));
            });
            if self.cracking {
                ui.ctx().request_repaint();
                let to = (self.next_guess + PER_FRAME).min(1_000_000);
                self.found = crack(&p, self.next_guess, to);
                self.next_guess = self.found.map_or(to, |f| f + 1);
                if self.found.is_some() || self.next_guess >= 1_000_000 {
                    self.cracking = false;
                }
            }
            if let Some(f) = self.found {
                let stolen = s1(&tk_from_passkey(f), &p.srand, &p.mrand);
                ui.label(format!("passkey {f:06} reproduces Mconfirm. With it: STK = {}", hex(&stolen)));
                widgets::status(ui, false, if stolen == p.stk { "the eavesdropper has the same key as the two devices and can decrypt everything that follows" } else { "the recovered key differs" });
            }
        });

        widgets::formula(
            ui,
            "c1(k, r, ...) = AES(k, AES(k, r XOR p1) XOR p2)   (p1, p2: pairing packets, addresses)\n\
             s1(k, r1, r2) = AES(k, low half of r1 || low half of r2)\n\
             possible passkeys = 1 000 000, about 2^20; each guess costs two AES operations",
        );
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "crack" => self.cracking = true,
            "just-works" => {
                self.just_works = true;
                self.searched_for = (true, self.passkey);
                self.cracking = true;
            }
            "other-passkey" => {
                self.passkey = 31_415;
                self.searched_for = (false, self.passkey);
                self.cracking = true;
            }
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "ble-pairing",
        title: "Pairing and encryption",
        blurb: "Two devices agree on a secret key over the air. In the older method, anyone who recorded the pairing can work it out in seconds.",
        path: &["Wireless", "Bluetooth", "Bluetooth LE"],
        make: || Box::new(Pairing::default()),
        explain: Explanation {
            what: "The six boxes are the steps of LE legacy pairing between a phone and a sensor, with the actual numbers. Each step \
                says whether its value is sent over the air (and so can be recorded) or stays inside the devices.\n\n\
                The lower box is an eavesdropper who recorded the whole pairing with a cheap radio. Press the button to let it \
                guess.",
            why: "To encrypt a connection, both devices need the same secret [[key]]. But they have never met, and everything they \
                send can be overheard. [[Pairing]] is the procedure for agreeing on a key anyway.\n\n\
                In the original Bluetooth LE method, now called legacy pairing, the secret ingredient is a temporary key, TK, \
                that never goes over the air. With Passkey Entry, the sensor shows a 6-digit number and the user types it into \
                the phone; that number is the TK. With Just Works, there is no number and the TK is simply zero.\n\n\
                The devices then prove to each other that they hold the same TK without sending it. Each picks a random \
                number and first sends only a confirm value: the random number scrambled with the TK by [[AES]] encryption, \
                together with the pairing packets and both addresses. Then they reveal the random numbers. Each side can now \
                recompute the other's confirm value; if it fits, the other side must have used the same TK. Finally both mix \
                the TK and the two random numbers into a short-term key, STK, which encrypts the connection.\n\n\
                The weakness is in the size of the secret. A 6-digit passkey can take only a million values. An eavesdropper \
                who recorded the pairing has everything except the TK - and can simply try all million: for each guess, \
                compute the confirm value and compare it with the recorded one. That is two million AES operations, a matter \
                of seconds on any computer. The specification itself says so: Just Works offers no protection against \
                eavesdroppers, and Passkey Entry \"very limited\" protection.\n\n\
                This is why Bluetooth 4.2 introduced LE Secure Connections. There, the two devices first agree on a secret by \
                elliptic-curve Diffie-Hellman: a mathematical exchange in which both sides publish a value and each computes \
                the same secret from its own private number and the other's public value. An eavesdropper sees both public \
                values and still cannot compute the secret. The passkey is then only used to check that nobody is sitting in \
                the middle, not as the secret itself.",
            formulas: &[
                ("c1(k, r, ...) = AES(k, AES(k, r XOR p1) XOR p2)", "With TK = 0 and the specification's example values, c1 = 1E1E3FEF878988EAD2A74DC5BEF13B86."),
                ("possible passkeys = 10^6 = about 2^20", "Twenty bits of secret. A modern key has 128: 2^108 times as many possibilities."),
            ],
            try_this: &[
                Experiment {
                    action: "Let the eavesdropper guess.",
                    watch: "the progress bar reaching 123 456 within a few seconds, and the recovered key matching the devices' key digit for digit.",
                    preset: Some("crack"),
                },
                Experiment {
                    action: "Switch to Just Works and guess.",
                    watch: "the very first guess, 000000, being right. Without a passkey there is nothing to guess.",
                    preset: Some("just-works"),
                },
                Experiment {
                    action: "Choose a different passkey, for example 031415.",
                    watch: "every confirm value and the key changing completely - and the search still succeeding. Choosing the number well does not help; there are simply too few of them.",
                    preset: Some("other-passkey"),
                },
            ],
            leaves_out: "Only the arithmetic of legacy pairing is shown. The pairing packets and addresses are the specification's \
                example values. After pairing, devices usually exchange long-term keys (encrypted with the STK) so that they need \
                not pair again; an eavesdropper who has the STK reads those too. The encryption of the connection itself (AES in \
                CCM mode) is not shown. LE Secure Connections is explained but not computed: it needs elliptic-curve arithmetic \
                that this project does not contain. An active attacker who sits in the middle during pairing is a further threat \
                that only Secure Connections with passkey or numeric comparison defends against.",
            refs: &[
                "Bluetooth Core Specification 5.4, Vol 3 Part H: c1 (2.2.3) and s1 (2.2.4) with their example values, checked bit for bit in sim-core; STK = s1(TK, LP_RAND_R, LP_RAND_I); passkey 019655 gives TK 0x...4CC7; Just Works \"provides no protection against eavesdroppers\", Passkey Entry \"very limited protection\", a 6-digit passkey \"approximately 20 bits of entropy\".",
                "AES-128 from crypto.rs, tested against FIPS-197 vectors. Teaching code, not hardened.",
            ],
        },
    }
}
