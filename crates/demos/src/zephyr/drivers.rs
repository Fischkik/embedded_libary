use crate::prelude::*;
use sim_core::zephyr::{SensorValue, TempSensor};

const APP_CODE: &str = "/* the application - identical for both sensors */\n\
const struct device *const dev = DEVICE_DT_GET(DT_ALIAS(temp0));\n\
struct sensor_value val;\n\
\n\
if (!device_is_ready(dev)) {\n    return;\n}\n\
sensor_sample_fetch(dev);                                  /* step 1: measure */\n\
sensor_channel_get(dev, SENSOR_CHAN_AMBIENT_TEMP, &val);   /* step 2: read the result */";

pub struct Drivers {
    sensor: TempSensor,
    celsius: f64,
    bus_ready: bool,
}

impl Default for Drivers {
    fn default() -> Self {
        Self { sensor: TempSensor::DigitalI2c, celsius: 25.06, bus_ready: true }
    }
}

impl Demo for Drivers {
    fn ui(&mut self, ui: &mut Ui, _cx: &mut DemoCx) {
        let pal = theme::palette(ui);
        widgets::group(ui, "The board", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Its temperature sensor is:");
                ui.selectable_value(&mut self.sensor, TempSensor::DigitalI2c, "a digital chip on the I2C bus").on_hover_text("The chip measures by itself; the driver asks it for the result over a two-wire bus.");
                ui.selectable_value(&mut self.sensor, TempSensor::AnalogAdc, "an analog sensor on an ADC pin").on_hover_text("The sensor puts out a voltage that rises with temperature; the driver measures that voltage with the microcontroller's ADC.");
            });
            ui.horizontal_wrapped(|ui| {
                widgets::wrap(ui, "temp", |ui| ui.add(egui::Slider::new(&mut self.celsius, -20.0..=60.0).text("real temperature").suffix(" °C").fixed_decimals(2)).on_hover_text("The temperature at the sensor."));
                ui.checkbox(&mut self.bus_ready, "the sensor started up properly").on_hover_text("Untick to see what the application gets when the driver's start-up failed, for example because its bus was not ready (see the boot demo).");
            });
        });
        widgets::group(ui, "The application", |ui| widgets::formula(ui, APP_CODE));

        let sample = self.sensor.fetch(self.celsius);
        let value = SensorValue::from_f64(sample.celsius);
        let digital = self.sensor == TempSensor::DigitalI2c;
        let (fetch_fn, get_fn) = if digital { ("tmp_i2c_sample_fetch", "tmp_i2c_channel_get") } else { ("tmp_adc_sample_fetch", "tmp_adc_channel_get") };
        widgets::group(ui, "The device behind \"dev\": three parts", |ui| {
            egui::Grid::new("device").striped(true).num_columns(3).show(ui, |ui| {
                for (part, lives, content) in [
                    ("config", "in flash, fixed at build time", if digital { "bus = i2c0, address = 0x48".to_owned() } else { "ADC channel 3, reference 3.3 V".to_owned() }),
                    ("data", "in RAM, changes while running", if self.bus_ready { format!("last raw reading = {}", sample.raw) } else { "nothing: the driver never started".to_owned() }),
                    ("api", "in flash: a table of the driver's functions", format!("sample_fetch -> {fetch_fn}()\nchannel_get  -> {get_fn}()")),
                ] {
                    ui.label(RichText::new(part).monospace().strong());
                    ui.add(egui::Label::new(RichText::new(lives).color(pal.dim)).extend());
                    ui.add(egui::Label::new(RichText::new(content).monospace()).extend());
                    ui.end_row();
                }
            });
        });

        ui.label(RichText::new("What happens when the application runs").strong());
        let path = if !self.bus_ready {
            "device_is_ready(dev) is false -> the application returns.\nWithout that check, sensor_sample_fetch(dev) would return an error and val would hold nonsense.".to_owned()
        } else if digital {
            format!(
                "sensor_sample_fetch(dev)   -> looks in dev's api table -> {fetch_fn}()\n\
                 \x20  driver: read 2 bytes from I2C address 0x48  ->  raw = {}  (steps of 0.0625 °C)\n\
                 sensor_channel_get(dev, ...) -> looks in dev's api table -> {get_fn}()\n\
                 \x20  driver: {} x 0.0625 = {:.4} °C  ->  val1 = {}, val2 = {}",
                sample.raw, sample.raw, sample.celsius, value.val1, value.val2
            )
        } else {
            format!(
                "sensor_sample_fetch(dev)   -> looks in dev's api table -> {fetch_fn}()\n\
                 \x20  driver: one ADC conversion on channel 3  ->  raw = {}  (of 4096 steps over 3.3 V)\n\
                 sensor_channel_get(dev, ...) -> looks in dev's api table -> {get_fn}()\n\
                 \x20  driver: {} x 3.3 V / 4096 = {:.1} mV;  ({:.1} mV - 500 mV) / 10 mV per °C = {:.4} °C  ->  val1 = {}, val2 = {}",
                sample.raw,
                sample.raw,
                sample.raw as f64 * 3.3 / 4096.0 * 1000.0,
                sample.raw as f64 * 3.3 / 4096.0 * 1000.0,
                sample.celsius,
                value.val1,
                value.val2
            )
        };
        widgets::formula(ui, path);
        widgets::status(
            ui,
            self.bus_ready,
            if self.bus_ready {
                format!("the application reads {:.4} °C (real: {:.2} °C); this sensor resolves steps of {:.4} °C", sample.celsius, self.celsius, self.sensor.step_celsius())
            } else {
                "the device is not ready: the application gets no temperature at all".to_owned()
            },
        );
        if self.bus_ready {
            widgets::formula(ui, format!("sensor_value: temperature = val1 + val2 / 1 000 000 = {} + {} / 1 000 000 = {:.6} °C", value.val1, value.val2, value.val1 as f64 + value.val2 as f64 / 1e6));
        }
    }

    fn apply_preset(&mut self, preset: &str) {
        *self = Self::default();
        match preset {
            "analog" => self.sensor = TempSensor::AnalogAdc,
            "cold" => self.celsius = -3.25,
            "not-ready" => self.bus_ready = false,
            _ => {}
        }
    }
}

pub fn entry() -> DemoEntry {
    DemoEntry {
        id: "zephyr-drivers",
        title: "Device drivers and the device model",
        blurb: "Two very different temperature sensors, one application that cannot tell them apart.",
        path: &["Zephyr"],
        make: || Box::new(Drivers::default()),
        explain: Explanation {
            what: "At the top you choose which temperature sensor the board has and how warm it is. The application in the box \
                below never changes. Under it is the device the application talks to, split into its three parts, and then \
                the path of the two calls: which function of which [[driver]] really runs, what it reads from the hardware, \
                and how that becomes a temperature.",
            why: "A digital temperature chip and an analog one have almost nothing in common. One is asked for a number over a \
                bus. The other produces a voltage that has to be measured and converted. Firmware written against either of \
                them would have to be rewritten for the other.\n\n\
                [[Zephyr]] puts a fixed layer in between. For each kind of hardware - sensors, serial ports, pins, displays - \
                there is one set of functions, the same for every chip of that kind. For sensors the set is small: fetch a \
                sample, then get a channel of it, such as the ambient temperature. The application only ever calls these.\n\n\
                Behind them, every piece of hardware is represented by a device, and a device has three parts. Config is the \
                unchanging description: which bus, which address, which pin. It comes from the [[devicetree]] and lives in \
                [[flash]]. Data is the driver's working memory for this one device, in RAM. And api is a table with the \
                addresses of this driver's own functions. When the application calls the generic fetch function, Zephyr \
                looks into the device's table and calls whatever the driver has put there. Swapping the sensor means \
                swapping the table, and nothing above it notices.\n\n\
                The result comes back in a fixed form as well: two whole numbers, the part before the decimal point and \
                the part after it in millionths. This avoids floating-point arithmetic, which many microcontrollers \
                cannot do in hardware.\n\n\
                Which driver is built in, and for which device, follows from the devicetree: a node that names a chip the \
                driver knows makes the build create a device for it.",
            formulas: &[
                ("value = val1 + val2 / 1 000 000", "25.0625 °C is stored as val1 = 25 and val2 = 62500."),
                ("digital chip: temperature = raw x 0.0625 °C        analog sensor: temperature = (voltage - 500 mV) / 10 mV per °C", "Raw 401 gives 25.0625 °C. A reading of 749 mV gives 24.9 °C."),
            ],
            try_this: &[
                Experiment {
                    action: "Switch the board to the analog sensor.",
                    watch: "the application box staying exactly as it is, while the api table, the hardware access and the arithmetic underneath all change - and the answer becoming coarser, in steps of 0.08 °C.",
                    preset: Some("analog"),
                },
                Experiment {
                    action: "With the digital chip, set the temperature to -3.25 °C.",
                    watch: "val1 = -3 and val2 = -250000: for negative values both parts are negative.",
                    preset: Some("cold"),
                },
                Experiment {
                    action: "Untick \"the sensor started up properly\".",
                    watch: "the data part staying empty and device_is_ready stopping the application before it reads nonsense.",
                    preset: Some("not-ready"),
                },
            ],
            leaves_out: "The two sensors and the names of their driver functions are made up, in the style of real ones; the \
                alias temp0 is an example too. Real drivers also report errors from each call, and offer more: several \
                channels, measurement ranges, triggers that call the application when a threshold is crossed. The fetch call \
                may wait for the hardware, so it has to be made from a thread. How the generic call finds the table - a \
                pointer in the device structure - is all there is to it; there is no searching at run time.",
            refs: &[
                "Zephyr documentation, Device Driver Model: the config, data and api members of a device; \"The api struct maps generic subsystem APIs to the device-specific implementations in the driver.\"",
                "Zephyr documentation, Sensors > Fetch and Get: DEVICE_DT_GET, device_is_ready, sensor_sample_fetch, sensor_channel_get; API reference for struct sensor_value (val2 in millionths).",
            ],
        },
    }
}
