//! Translates the executor's actions into RustDesk input events. The same
//! `Action` values drive RDP, so the two transports stay interchangeable.
use crate::remote::Action;
use crate::rustdesk::proto::{self, key_event, ControlKey, KeyboardMode};

pub const TYPE_MOVE: i32 = 0;
pub const TYPE_DOWN: i32 = 1;
pub const TYPE_UP: i32 = 2;
pub const TYPE_WHEEL: i32 = 3;
pub const BUTTON_LEFT: i32 = 0x01;
pub const BUTTON_RIGHT: i32 = 0x02;

/// The peer reads the button from the high bits and the event from the low three.
pub fn mask(button: i32, event: i32) -> i32 {
    button << 3 | event
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Mouse { mask: i32, x: i32, y: i32 },
    Key(proto::KeyEvent),
}

fn control_key(name: &str) -> Option<ControlKey> {
    Some(match name {
        "enter" => ControlKey::Return,
        "tab" => ControlKey::Tab,
        "esc" | "escape" => ControlKey::Escape,
        "backspace" => ControlKey::Backspace,
        "delete" => ControlKey::Delete,
        "space" => ControlKey::Space,
        "insert" => ControlKey::Insert,
        "up" => ControlKey::UpArrow,
        "down" => ControlKey::DownArrow,
        "left" => ControlKey::LeftArrow,
        "right" => ControlKey::RightArrow,
        "home" => ControlKey::Home,
        "end" => ControlKey::End,
        "pageup" => ControlKey::PageUp,
        "pagedown" => ControlKey::PageDown,
        "f1" => ControlKey::F1,
        "f2" => ControlKey::F2,
        "f3" => ControlKey::F3,
        "f4" => ControlKey::F4,
        "f5" => ControlKey::F5,
        "f6" => ControlKey::F6,
        "f7" => ControlKey::F7,
        "f8" => ControlKey::F8,
        "f9" => ControlKey::F9,
        "f10" => ControlKey::F10,
        "f11" => ControlKey::F11,
        "f12" => ControlKey::F12,
        _ => return None,
    })
}

fn modifier(name: &str) -> Option<ControlKey> {
    Some(match name {
        "ctrl" => ControlKey::Control,
        "alt" => ControlKey::Alt,
        "shift" => ControlKey::Shift,
        "win" | "meta" => ControlKey::Meta,
        _ => return None,
    })
}

/// The key the shortcut is actually about, as a layout key or a named key.
fn primary(name: &str, modifiers: Vec<i32>, down: bool) -> Result<proto::KeyEvent, String> {
    let union = match control_key(name) {
        Some(key) => key_event::Union::ControlKey(key as i32),
        None => {
            let mut characters = name.chars();
            match (characters.next(), characters.next()) {
                // A layout key, so the peer applies held modifiers as a shortcut.
                (Some(character), None) => key_event::Union::Chr(character as u32),
                _ => return Err(format!("Tecla não suportada: {name}")),
            }
        }
    };
    Ok(proto::KeyEvent {
        down,
        press: false,
        union: Some(union),
        modifiers,
        mode: KeyboardMode::Legacy as i32,
    })
}

/// Typed text goes through as Unicode so the peer's keyboard layout cannot
/// change which characters arrive.
fn typed(character: char) -> proto::KeyEvent {
    proto::KeyEvent {
        down: true,
        press: true,
        union: Some(key_event::Union::Unicode(character as u32)),
        modifiers: Vec::new(),
        mode: KeyboardMode::Legacy as i32,
    }
}

fn click(x: u32, y: u32, button: i32, times: u32, width: u32, height: u32) -> Result<Vec<Input>, String> {
    if x >= width || y >= height {
        return Err("O clique ficou fora da imagem.".into());
    }
    let (x, y) = (x as i32, y as i32);
    let mut events = vec![Input::Mouse { mask: mask(0, TYPE_MOVE), x, y }];
    for _ in 0..times {
        events.push(Input::Mouse { mask: mask(button, TYPE_DOWN), x, y });
        events.push(Input::Mouse { mask: mask(button, TYPE_UP), x, y });
    }
    Ok(events)
}

pub fn translate(action: &Action, width: u32, height: u32) -> Result<Vec<Input>, String> {
    match action {
        Action::Click { x, y } => click(*x, *y, BUTTON_LEFT, 1, width, height),
        Action::DoubleClick { x, y } => click(*x, *y, BUTTON_LEFT, 2, width, height),
        Action::RightClick { x, y } => click(*x, *y, BUTTON_RIGHT, 1, width, height),
        Action::TypeText { text } => {
            if text.is_empty() || text.chars().count() > 400 || text.contains('\0') {
                return Err("Digite no máximo 400 caracteres por ação.".into());
            }
            Ok(text.chars().map(|c| Input::Key(typed(c))).collect())
        }
        Action::Key { keys } => {
            if keys.is_empty() || keys.len() > 5 {
                return Err("Combinação de teclas inválida.".into());
            }
            let normalized: Vec<String> = keys.iter().map(|k| k.to_ascii_lowercase()).collect();
            let unique: std::collections::HashSet<_> = normalized.iter().collect();
            let main: Vec<&String> = normalized.iter().filter(|k| modifier(k).is_none()).collect();
            if main.len() != 1 || unique.len() != keys.len() {
                return Err("keys aceita um atalho por ação, não uma sequência. Envie cada atalho/Enter em uma ação separada.".into());
            }
            let modifiers: Vec<i32> = normalized
                .iter()
                .filter_map(|k| modifier(k))
                .map(|k| k as i32)
                .collect();
            Ok(vec![
                Input::Key(primary(main[0], modifiers.clone(), true)?),
                Input::Key(primary(main[0], modifiers, false)?),
            ])
        }
        Action::Scroll { direction, amount } => {
            if !(1..=10).contains(amount) || !["up", "down"].contains(&direction.as_str()) {
                return Err("Rolagem inválida.".into());
            }
            let step = if direction == "up" { 1 } else { -1 };
            Ok((0..*amount)
                .map(|_| Input::Mouse { mask: mask(0, TYPE_WHEEL), x: 0, y: step })
                .collect())
        }
        Action::Inspect { .. }
        | Action::Wait { .. }
        | Action::StepDone { .. }
        | Action::Blocked { .. } => Err("Essa ação não envia entrada ao Windows.".into()),
    }
}

/// Everything a paused session must let go of, so no button or modifier stays
/// held on the remote machine.
pub fn release() -> Vec<Input> {
    let mut events = vec![
        Input::Mouse { mask: mask(BUTTON_LEFT, TYPE_UP), x: 0, y: 0 },
        Input::Mouse { mask: mask(BUTTON_RIGHT, TYPE_UP), x: 0, y: 0 },
    ];
    for key in [
        ControlKey::Control,
        ControlKey::Alt,
        ControlKey::Shift,
        ControlKey::Meta,
    ] {
        events.push(Input::Key(proto::KeyEvent {
            down: false,
            press: false,
            union: Some(key_event::Union::ControlKey(key as i32)),
            modifiers: Vec::new(),
            mode: KeyboardMode::Legacy as i32,
        }));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Action {
        Action::Key {
            keys: names.iter().map(|k| (*k).to_string()).collect(),
        }
    }

    #[test]
    fn clicks_move_first_and_stay_inside_the_picture() {
        let events = translate(&Action::Click { x: 10, y: 20 }, 1280, 800).unwrap();
        assert_eq!(
            events,
            vec![
                Input::Mouse { mask: 0, x: 10, y: 20 },
                Input::Mouse { mask: 9, x: 10, y: 20 },
                Input::Mouse { mask: 10, x: 10, y: 20 },
            ]
        );
        // Right button is 2, so down is 0b10_001 and up 0b10_010.
        let right = translate(&Action::RightClick { x: 1, y: 1 }, 8, 8).unwrap();
        assert_eq!(right[1], Input::Mouse { mask: 17, x: 1, y: 1 });
        assert_eq!(right[2], Input::Mouse { mask: 18, x: 1, y: 1 });
        assert_eq!(translate(&Action::DoubleClick { x: 0, y: 0 }, 8, 8).unwrap().len(), 5);
        assert!(translate(&Action::Click { x: 1280, y: 0 }, 1280, 800).is_err());
        assert!(translate(&Action::Click { x: 0, y: 800 }, 1280, 800).is_err());
    }

    #[test]
    fn shortcuts_press_and_release_with_their_modifiers() {
        let events = translate(&keys(&["ctrl", "s"]), 8, 8).unwrap();
        let Input::Key(down) = &events[0] else { panic!() };
        let Input::Key(up) = &events[1] else { panic!() };
        assert_eq!(down.modifiers, vec![ControlKey::Control as i32]);
        // A layout key, so the peer treats it as Ctrl+S and not as typed text.
        assert_eq!(down.union, Some(key_event::Union::Chr('s' as u32)));
        assert!(down.down && !up.down);
        assert_eq!(up.union, down.union);
        let enter = translate(&keys(&["enter"]), 8, 8).unwrap();
        let Input::Key(event) = &enter[0] else { panic!() };
        assert_eq!(
            event.union,
            Some(key_event::Union::ControlKey(ControlKey::Return as i32))
        );
        assert!(event.modifiers.is_empty());
    }

    #[test]
    fn sequences_and_unknown_keys_are_refused_rather_than_half_sent() {
        for action in [
            keys(&["ctrl", "s", "enter"]),
            keys(&["ctrl", "ctrl"]),
            keys(&["ctrl"]),
            keys(&[]),
            keys(&["teclaquenaoexiste"]),
        ] {
            assert!(translate(&action, 8, 8).is_err(), "{action:?}");
        }
    }

    #[test]
    fn typed_text_travels_as_unicode_and_stays_bounded() {
        let events = translate(
            &Action::TypeText {
                text: "olá".into(),
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(events.len(), 3);
        let Input::Key(event) = &events[1] else { panic!() };
        assert_eq!(event.union, Some(key_event::Union::Unicode('l' as u32)));
        assert!(event.modifiers.is_empty(), "texto não deve virar atalho");
        assert!(translate(&Action::TypeText { text: String::new() }, 8, 8).is_err());
        assert!(translate(
            &Action::TypeText {
                text: "x".repeat(401)
            },
            8,
            8
        )
        .is_err());
    }

    #[test]
    fn scrolling_has_a_direction_and_a_bound() {
        let up = translate(
            &Action::Scroll {
                direction: "up".into(),
                amount: 3,
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(up.len(), 3);
        assert_eq!(up[0], Input::Mouse { mask: TYPE_WHEEL, x: 0, y: 1 });
        let down = translate(
            &Action::Scroll {
                direction: "down".into(),
                amount: 1,
            },
            8,
            8,
        )
        .unwrap();
        assert_eq!(down[0], Input::Mouse { mask: TYPE_WHEEL, x: 0, y: -1 });
        for bad in [("up", 0), ("up", 11), ("sideways", 1)] {
            assert!(translate(
                &Action::Scroll {
                    direction: bad.0.into(),
                    amount: bad.1
                },
                8,
                8
            )
            .is_err());
        }
    }

    #[test]
    fn pausing_releases_every_button_and_modifier() {
        let events = release();
        assert_eq!(
            events[0],
            Input::Mouse { mask: mask(BUTTON_LEFT, TYPE_UP), x: 0, y: 0 }
        );
        assert!(events[2..].iter().all(|event| matches!(
            event,
            Input::Key(proto::KeyEvent { down: false, .. })
        )));
        assert_eq!(events.len(), 6);
    }

    /// The executor does not know which transport it is driving, so an action
    /// it may send over RDP must be equally acceptable over RustDesk, and an
    /// action RDP refuses must be refused here too.
    #[test]
    fn both_transports_accept_and_refuse_exactly_the_same_actions() {
        let text = |value: &str| Action::TypeText { text: value.into() };
        for action in [
            Action::Click { x: 0, y: 0 },
            Action::Click { x: 1279, y: 799 },
            Action::Click { x: 1280, y: 0 },
            Action::Click { x: 0, y: 800 },
            Action::DoubleClick { x: 5, y: 5 },
            Action::RightClick { x: 5, y: 5 },
            text("olá"),
            text(""),
            text(&"x".repeat(400)),
            text(&"x".repeat(401)),
            keys(&["ctrl", "s"]),
            keys(&["enter"]),
            keys(&["f5"]),
            keys(&["ctrl", "alt", "delete"]),
            keys(&["ctrl", "s", "enter"]),
            keys(&["ctrl", "ctrl"]),
            keys(&[]),
            keys(&["teclaquenaoexiste"]),
            Action::Scroll { direction: "up".into(), amount: 1 },
            Action::Scroll { direction: "down".into(), amount: 10 },
            Action::Scroll { direction: "down".into(), amount: 11 },
            Action::Scroll { direction: "sideways".into(), amount: 1 },
            Action::Wait { seconds: 1 },
            Action::StepDone { evidence: "x".into() },
            Action::Blocked { reason: "x".into() },
            Action::Inspect { x: 0, y: 0, width: 1, height: 1 },
        ] {
            assert_eq!(
                crate::remote::action_commands(&action, 1280, 800).is_ok(),
                translate(&action, 1280, 800).is_ok(),
                "os transportes divergiram em {action:?}"
            );
        }
    }

    #[test]
    fn actions_that_do_not_touch_windows_are_rejected_like_on_rdp() {
        for action in [
            Action::Wait { seconds: 1 },
            Action::StepDone { evidence: "x".into() },
            Action::Blocked { reason: "x".into() },
            Action::Inspect { x: 0, y: 0, width: 1, height: 1 },
        ] {
            assert!(translate(&action, 8, 8).is_err());
        }
    }
}
