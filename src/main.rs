use bidirectional_map::Bimap;
use futures_lite::future::block_on;
use gilrs::EventType::Disconnected;
use gilrs::{ev, Axis, Event, EventType, Gilrs};
use lazy_static::lazy_static;
use nusb::{Device, DeviceInfo, Interface};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

lazy_static! {
    static ref BTN_ASSOCIATION: Bimap<Button, gilrs::Button> = Bimap::from_hash_map(HashMap::from([
        (Button::A, ev::Button::East),
        (Button::B, ev::Button::South),
        (Button::Y, ev::Button::West),
        (Button::X, ev::Button::North),
        (Button::DPADRIGHT, ev::Button::DPadRight),
        (Button::DPADDOWN, ev::Button::DPadDown),
        (Button::DPADLEFT, ev::Button::DPadLeft),
        (Button::DPADUP, ev::Button::DPadUp),
        (Button::R1, ev::Button::RightTrigger),
        (Button::L1, ev::Button::LeftTrigger),
        (Button::R2, ev::Button::RightTrigger2),
        (Button::L2, ev::Button::LeftTrigger2),
        (Button::R3, ev::Button::RightThumb),
        (Button::L3, ev::Button::LeftThumb),
        (Button::START, ev::Button::Start),
        (Button::SELECT, ev::Button::Select),
        (Button::HOME, ev::Button::Mode),
        (Button::CAPTURE, ev::Button::Unknown)
    ]));
}

#[derive(Eq, PartialEq)]
#[derive(Hash, Debug, Copy, Clone)]
enum Button {
    A,
    B,
    X,
    Y,
    DPADRIGHT,
    DPADDOWN,
    DPADLEFT,
    DPADUP,
    R1,
    L1,
    R2,
    L2,
    R3,
    L3,
    START,
    SELECT,
    CAPTURE,
    HOME
}

#[derive(Debug, Copy, Clone)]
enum ButtonState {
    PRESSED,
    HELD,
    RELEASED
}

#[derive(Debug, Clone)]
struct ControllerState {
    button_states: HashMap<Button, ButtonState>,
    r_stick: (i32, i32),
    l_stick: (i32, i32),
    old_r_stick: (i32, i32),
    old_l_stick: (i32, i32),
    old_state: Option<HashMap<Button, ButtonState>>,
}

const fn get_button_name(button: Button) -> &'static str {
    match button {
        Button::A => "A",
        Button::B => "B",
        Button::X => "X",
        Button::Y => "Y",
        Button::DPADRIGHT => "DRIGHT",
        Button::DPADDOWN => "DDOWN",
        Button::DPADLEFT => "DLEFT",
        Button::DPADUP => "DUP",
        Button::R1 => "R",
        Button::L1 => "L",
        Button::R2 => "ZR",
        Button::L2 => "ZL",
        Button::R3 => "RSTICK",
        Button::L3 => "LSTICK",
        Button::START => "PLUS",
        Button::SELECT => "MINUS",
        Button::CAPTURE => "CAPTURE",
        Button::HOME => "HOME"
    }
}

impl ControllerState {
    fn new() -> ControllerState {
        ControllerState {
            r_stick: (0, 0),
            l_stick: (0, 0),
            old_l_stick: (0,0),
            old_r_stick: (0, 0),
            button_states: HashMap::new(),
            old_state: None
        }
    }

    fn set_button_states(&mut self, (new_button, new_state): (Button, ButtonState)) {
        let binding = self.clone().old_state.unwrap_or_default();
        let old_button_state = binding.get(&new_button);
        if let Some(reference) = self.button_states.get_mut(&new_button) {
            match *reference {
                ButtonState::PRESSED => {
                    match new_state {
                        ButtonState::PRESSED => {}
                        ButtonState::HELD => {}
                        ButtonState::RELEASED => {}
                    }
                }
                ButtonState::HELD => {
                    match new_state {
                        ButtonState::PRESSED => { *reference = ButtonState::PRESSED }
                        ButtonState::HELD => {}
                        ButtonState::RELEASED => { *reference = ButtonState::PRESSED}
                    }
                }
                ButtonState::RELEASED => {
                    match new_state {
                        ButtonState::PRESSED => { *reference = ButtonState::HELD }
                        ButtonState::HELD => { *reference = ButtonState::HELD }
                        ButtonState::RELEASED => {  }
                    }
                }
            }
        } else if let Some(old_state) = old_button_state {
            match *old_state {
                ButtonState::PRESSED => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); }
                        ButtonState::HELD => { self.button_states.insert(new_button, ButtonState::HELD); }
                        ButtonState::RELEASED => { self.button_states.insert(new_button, ButtonState::RELEASED); }
                    }
                }
                ButtonState::HELD => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); }
                        ButtonState::HELD => {  }
                        ButtonState::RELEASED => {
                            self.button_states.insert(new_button, ButtonState::RELEASED);
                        }
                    }
                }
                ButtonState::RELEASED => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); }
                        ButtonState::HELD => { self.button_states.insert(new_button, ButtonState::HELD); }
                        ButtonState::RELEASED => {}
                    }
                }
            }
        } else {
            self.button_states.insert(new_button, new_state);
        }
    }

    fn make_packets(self) -> Vec<String> {
        let mut packets: Vec<_> = self.button_states.iter().map(|(button, state)| {
            match state {
                ButtonState::PRESSED => {
                    format!("click {}", get_button_name(*button))
                }
                ButtonState::HELD => {
                    format!("press {}", get_button_name(*button))
                }
                ButtonState::RELEASED => {
                    format!("release {}", get_button_name(*button))
                }
            }
        }).collect();

        if self.old_r_stick != self.r_stick {
            packets.push(format!("setStick RIGHT {} {}", to_hex_string(self.r_stick.0), to_hex_string(self.r_stick.1)));
        }

        if self.old_l_stick != self.l_stick {
            packets.push(format!("setStick LEFT {} {}", to_hex_string(self.l_stick.0), to_hex_string(self.l_stick.1)));
        }

        packets
    }
}

fn to_hex_string(n: i32) -> String {
    // Check if the number is in the range of a 16-bit signed integer
    if !(-0x8000..=0x7FFF).contains(&n) {
        panic!("Number out of range for 16-bit signed integer");
    }

    // Format the number as a hexadecimal string
    if n < 0 {
        format!("-0x{:>04X}", -n) // For negative numbers
    } else {
        format!("0x{:>04X}", n) // For non-negative numbers
    }
}


fn get_axis_values(value: f32) -> i32 {
    let val = (value * 32767.) as i32;
    if val.abs() < 5000 {
        0
    } else {
        val
    }
}

fn get_switch_device_info() -> DeviceInfo {
    for device_info in nusb::list_devices().unwrap() {
        if device_info.vendor_id() == 0x057e && device_info.product_id() == 0x3000 {
            return device_info;
        }
    }

    panic!("Unable to find a switch device!");
}

fn get_device(device_info: DeviceInfo) -> Device {
    match device_info.open() {
        Ok(handle) => {
            println!("Opened switch device!");
            handle
        }
        Err(err) => {
            panic!("Failed to open switch device! Error: {}", err)
        }
    }
}

fn build_packets(data: Vec<String>) -> Vec<Vec<u8>> {
    data.iter().flat_map(|s| {
        [
            ((s.len() + 2) as i32).to_le_bytes().to_vec(),
            s.as_bytes().to_vec()
        ]
    }).collect()
}
fn write_packet(interface: &Interface, data: Vec<Vec<u8>>) {
    let mut queue = interface.bulk_out_queue(0x1);
    data.iter().for_each(|x| queue.submit(x.clone()));
    for _ in 0..data.len() {
        let _ = block_on(queue.next_complete());
    }
}

fn main() {



    let mut gilrs = Gilrs::new().unwrap();
    let mut active_gamepad = None;
    let mut exit = false;
    let mut controller_state = ControllerState::new();
    let mut interface = None;
    while !exit {
        let a = SystemTime::now();
        let wait_for = Duration::from_millis(30);

        while SystemTime::now().duration_since(a).unwrap_or(Duration::from_millis(0)).lt(&wait_for) {
            while let Some(Event { id, event, .. }) = gilrs.next_event() {
                if active_gamepad.is_none() {
                    active_gamepad = Some(id);
                    let device_info = get_switch_device_info();
                    let device = get_device(device_info);
                    device.reset().expect("cannot reset");
                    interface = Some(device.claim_interface(0).unwrap());
                } else if active_gamepad.unwrap() != id {
                    continue
                }

                match event {
                    Disconnected => {
                        exit = true;
                        break;
                    }
                    e => {
                        match e {
                            EventType::ButtonPressed(btn, _) => {
                                if let Some(switch_key) = BTN_ASSOCIATION.get_rev(&btn) {
                                    controller_state.set_button_states((*switch_key, ButtonState::HELD));
                                }
                            }
                            EventType::ButtonReleased(btn, _) => {
                                if let Some(switch_key) = BTN_ASSOCIATION.get_rev(&btn) {
                                    controller_state.set_button_states((*switch_key, ButtonState::RELEASED));
                                }
                            }
                            EventType::AxisChanged(axis, value, _) => {
                                match axis {
                                    Axis::LeftStickX => { controller_state.l_stick.0 = get_axis_values(value) }
                                    Axis::LeftStickY => { controller_state.l_stick.1 = get_axis_values(value) }
                                    Axis::RightStickX => { controller_state.r_stick.0 = get_axis_values(value) }
                                    Axis::RightStickY => { controller_state.r_stick.1 = get_axis_values(value) }
                                    _ => {}
                                }
                            }
                            EventType::ForceFeedbackEffectCompleted => {}
                            _ => {}
                        }
                    }
                }
            }
        }

        if !exit && active_gamepad.is_some() {
            let packet_strings = controller_state.clone().make_packets();
            if !packet_strings.is_empty() {
                let packets = build_packets(packet_strings);
                if let Some(ref i_face) = interface {
                    write_packet(i_face, packets);
                }
            }
        }
        controller_state.old_state = Some(controller_state.button_states.clone());
        controller_state.old_l_stick = controller_state.l_stick;
        controller_state.old_r_stick = controller_state.r_stick;
        controller_state.button_states = HashMap::new();
    }

}


