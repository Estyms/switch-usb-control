use bidirectional_map::Bimap;
use futures_lite::future::block_on;
use gilrs::EventType::Disconnected;
use gilrs::{ev, Axis, Event, EventType, Gilrs};
use lazy_static::lazy_static;
use nusb::{Device, DeviceInfo, Interface};
use std::collections::HashMap;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
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

enum ConnectionType {
    USB,
    INTERNET,
}

enum Connection {
    USB(Interface),
    INTERNET(TcpStream),
}

#[derive(Eq, PartialEq, Clone, Copy)]
#[derive(Hash, Debug)]
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
    HOME,
}

enum Stick {
    RIGHT,
    LEFT
}

#[derive(Debug, Copy, Clone)]
enum ButtonState {
    PRESSED,
    HELD,
    RELEASED,
}

#[derive(Debug)]
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
            old_l_stick: (0, 0),
            old_r_stick: (0, 0),
            button_states: HashMap::new(),
            old_state: None,
        }
    }

    fn get_old_state(&self) -> HashMap<Button, ButtonState> {
        let x = &self.old_state;
        x.clone().unwrap_or_default()
    }

    fn set_button_states(&mut self, (new_button, new_state): (Button, ButtonState)) -> (Button, ButtonState) {
        let binding = self.get_old_state();
        let old_button_state = binding.get(&new_button);
        if let Some(reference) = self.button_states.get_mut(&new_button) {
            match *reference {
                ButtonState::PRESSED => {
                    match new_state {
                        ButtonState::PRESSED => {(new_button, *reference)},
                        ButtonState::HELD => {(new_button, *reference)}
                        ButtonState::RELEASED => {(new_button, *reference)}
                    }
                }
                ButtonState::HELD => {
                    match new_state {
                        ButtonState::PRESSED => { *reference = ButtonState::PRESSED; (new_button, *reference)}
                        ButtonState::HELD => {(new_button, *reference)}
                        ButtonState::RELEASED => { *reference = ButtonState::PRESSED; (new_button, *reference)}
                    }
                }
                ButtonState::RELEASED => {
                    match new_state {
                        ButtonState::PRESSED => { *reference = ButtonState::HELD; (new_button, *reference) }
                        ButtonState::HELD => { *reference = ButtonState::HELD; (new_button, *reference) }
                        ButtonState::RELEASED => {(new_button, *reference)}
                    }
                }
            }
        } else if let Some(old_state) = old_button_state {
            match *old_state {
                ButtonState::PRESSED => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); (new_button, ButtonState::PRESSED)}
                        ButtonState::HELD => { self.button_states.insert(new_button, ButtonState::HELD); (new_button, ButtonState::HELD)}
                        ButtonState::RELEASED => { self.button_states.insert(new_button, ButtonState::RELEASED); (new_button, ButtonState::RELEASED)}
                    }
                }
                ButtonState::HELD => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); (new_button, ButtonState::PRESSED)}
                        ButtonState::HELD => {(new_button, ButtonState::HELD)}
                        ButtonState::RELEASED => {
                            self.button_states.insert(new_button, ButtonState::RELEASED);
                            (new_button, ButtonState::RELEASED)
                        }
                    }
                }
                ButtonState::RELEASED => {
                    match new_state {
                        ButtonState::PRESSED => { self.button_states.insert(new_button, ButtonState::PRESSED); (new_button, ButtonState::PRESSED)}
                        ButtonState::HELD => { self.button_states.insert(new_button, ButtonState::HELD); (new_button, ButtonState::HELD)}
                        ButtonState::RELEASED => {(new_button, ButtonState::RELEASED)}
                    }
                }
            }
        } else {
            self.button_states.insert(new_button, new_state);
            (new_button, new_state)
        }
    }


    fn make_packets(&self) -> Vec<String> {
        let mut packets: Vec<_> = self.button_states.iter().map(|(button, state)| make_packet_for_button_state(*button, *state)).collect();

        if self.old_r_stick != self.r_stick {
            packets.push(make_packet_for_stick(Stick::RIGHT, self.r_stick))
        }

        if self.old_l_stick != self.l_stick {
            packets.push(make_packet_for_stick(Stick::LEFT, self.l_stick))
        }

        packets
    }
}

fn make_packet_for_stick(stick: Stick, value: (i32, i32)) -> String {
    match stick {
        Stick::RIGHT => format!("setStick RIGHT {} {}", to_hex_string(value.0), to_hex_string(value.1)),
        Stick::LEFT => format!("setStick LEFT {} {}", to_hex_string(value.0), to_hex_string(value.1))
    }
}

fn make_packet_for_button_state(button: Button, state: ButtonState) -> String {
    match state {
        ButtonState::PRESSED => {
            format!("click {}", get_button_name(button))
        }
        ButtonState::HELD => {
            format!("press {}", get_button_name(button))
        }
        ButtonState::RELEASED => {
            format!("release {}", get_button_name(button))
        }
    }
}


fn to_hex_string(n: i32) -> String {
    if !(-0x8000..=0x7FFF).contains(&n) {
        panic!("Number out of range for 16-bit signed integer");
    }

    if n < 0 {
        format!("-0x{:>04X}", -n)
    } else {
        format!("0x{:>04X}", n)
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
    let data_len = data.len();
    data.into_iter().for_each(|x| queue.submit(x));
    for _ in 0..data_len {
        let _ = block_on(queue.next_complete());
    }
}

fn process_button_action(controller_state: &mut ControllerState, btn: &gilrs::Button, state: ButtonState) {
    if let Some(switch_key) = BTN_ASSOCIATION.get_rev(btn) {
        controller_state.set_button_states((*switch_key, state));
    }
}

fn input_ip_address() -> SocketAddr {
    let ip = inquire::Text::new("Enter the IP Address of the switch")
        .prompt()
        .expect("Invalid IP");

    let port = inquire::Text::new("Enter the port of the switch")
        .prompt()
        .expect("Invalid Port");

    SocketAddr::from((ip.as_str().parse::<Ipv4Addr>().expect("Invalid IP"), port.parse::<u16>().expect("Invalid port number")))
}

fn main() {
    let ans = inquire::Select::new("What kind of connection do you want?", vec!["Internet", "USB"]).prompt().expect("No connection type selected");

    let connection_type = match ans {
        "Internet" => ConnectionType::INTERNET,
        "USB" => ConnectionType::USB,
        _ => panic!("Unknown connection type!")
    };

    let mut connection = match connection_type {
        ConnectionType::USB => {
            let device_info = get_switch_device_info();
            let device = get_device(device_info);
            device.reset().expect("cannot reset");
            Connection::USB(device.claim_interface(0).unwrap())
        }
        ConnectionType::INTERNET => {
            Connection::INTERNET(TcpStream::connect(input_ip_address()).expect("Cannot connect to switch"))
        }
    };

    println!("Successfully connected to switch device!");

    println!("Please connect and press a button on your controller");
    let mut gilrs = Gilrs::new().unwrap();
    let mut active_gamepad = None;
    let mut exit = false;
    let mut controller_state = ControllerState::new();
    while !exit {
        let a = SystemTime::now();
        let wait_for: Duration = match connection_type {
            ConnectionType::USB => Duration::from_millis(66),
            ConnectionType::INTERNET => Duration::from_millis(100)
        };

        while SystemTime::now().duration_since(a).unwrap_or(Duration::from_millis(0)).lt(&wait_for) {
            while let Some(Event { id, event, .. }) = gilrs.next_event() {
                if active_gamepad.is_none() {
                    active_gamepad = Some(id);
                    println!("Controller connected !");
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
                                process_button_action(&mut controller_state, &btn, ButtonState::HELD);
                            }
                            EventType::ButtonReleased(btn, _) => {
                                process_button_action(&mut controller_state, &btn, ButtonState::RELEASED);
                            }
                            EventType::AxisChanged(axis, value, _) => {
                                match axis {
                                    Axis::LeftStickX => { controller_state.l_stick.0 = get_axis_values(value)}
                                    Axis::LeftStickY => { controller_state.l_stick.1 = get_axis_values(value) }
                                    Axis::RightStickX => { controller_state.r_stick.0 = get_axis_values(value)}
                                    Axis::RightStickY => { controller_state.r_stick.1 = get_axis_values(value)}
                                    _ => { }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if !exit && active_gamepad.is_some() {
            use Connection::*;
            let packet_strings = controller_state.make_packets();
            if !packet_strings.is_empty() {
                match connection {
                    USB(ref interface) => {
                        let packets = build_packets(packet_strings);
                        write_packet(interface, packets);
                    }
                    INTERNET(ref mut socket) => {
                        packet_strings.iter()
                            .map(|s| format!("{}\r\n", s))
                            .for_each(|p| {
                                socket.write_all(p.as_bytes()).expect("Unable to send packet");
                            });
                    }
                }
            }
        }

        //Crappy but oh well
        controller_state.old_state = Some(controller_state.button_states);
        controller_state.old_l_stick = controller_state.l_stick;
        controller_state.old_r_stick = controller_state.r_stick;
        controller_state.button_states = HashMap::new();
    }
}




