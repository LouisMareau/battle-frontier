//! What an event does, one action a line. Plain words, so a script reads like stage
//! directions:
//!
//! ```text
//! say Bo: Nice day for a walk.
//! ask Sure is -> yes | Not really -> no
//! label yes
//! say Bo: Told you.
//! set met_bo
//! goto end
//! label no
//! move bo 12,8 12,11
//! face bo player
//! say Bo: Then come along.
//! label end
//! ```
//!
//! The actions: `say [Who:] text`, `ask text -> label | text -> label`, `label name`, `goto
//! name`, `if flag goto name`, `unless flag goto name`, `set flag [value]`, `add flag
//! value`, `clear flag`, `move who x,z [x,z ...]`, `face who n|s|e|w|player|<actor>`, `wait
//! seconds`, `show who`, `hide who`, `sound file [volume]`, `camera x,z|who`, `warp map
//! x,z`, `battle encounter`, `end`. `who` is an actor's id or `player`. Blank lines and
//! lines starting with `#` are skipped.

/// One action of a script.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Say { who: String, text: String },
    /// (what's offered, the label it goes to)
    Ask(Vec<(String, String)>),
    Label(String),
    Goto(String),
    /// `set`: jump when the flag is set (`if`) or when it isn't (`unless`)
    If { flag: String, label: String, set: bool },
    Set { flag: String, value: i64 },
    Add { flag: String, value: i64 },
    Clear(String),
    Move { who: String, cells: Vec<[i32; 2]> },
    Face { who: String, to: String },
    Wait(f64),
    Show(String),
    Hide(String),
    Sound { file: String, volume: f64 },
    Camera(Target),
    Warp { map: String, at: [i32; 2] },
    Battle(String),
    /// an item given to the player (into the Bag; a bigger bag grows it)
    Give(String),
    End,
}

/// Somewhere a camera can go.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Cell([i32; 2]),
    Who(String),
}

fn cell(word: &str) -> Result<[i32; 2], String> {
    let (x, z) = word.split_once(',').ok_or_else(|| format!("\"{word}\" isn't a cell (x,z)"))?;
    match (x.trim().parse(), z.trim().parse()) {
        (Ok(x), Ok(z)) => Ok([x, z]),
        _ => Err(format!("\"{word}\" isn't a cell (x,z)")),
    }
}

fn name_ok(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '/' || c == '.')
}

/// One line as an action (None for a blank line or a comment).
pub fn parse_line(line: &str) -> Result<Option<Action>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let (verb, rest) = match line.split_once(char::is_whitespace) {
        Some((v, r)) => (v, r.trim()),
        None => (line, ""),
    };
    let words: Vec<&str> = rest.split_whitespace().collect();
    let need = |n: usize, what: &str| if words.len() < n { Err(format!("{verb}: {what}")) } else { Ok(()) };
    let who = |w: &str| if w == "player" || name_ok(w) { Ok(w.to_string()) } else { Err(format!("{verb}: \"{w}\" isn't an actor id")) };
    let action = match verb {
        "say" => {
            if rest.is_empty() {
                return Err("say: nothing to say".into());
            }
            match rest.split_once(':') {
                Some((w, t)) if !w.contains(' ') && !w.is_empty() && !t.trim().is_empty() => Action::Say { who: w.trim().to_string(), text: t.trim().to_string() },
                _ => Action::Say { who: String::new(), text: rest.to_string() },
            }
        }
        "ask" => {
            let mut options = vec![];
            for part in rest.split('|') {
                let (text, label) = part.split_once("->").ok_or_else(|| format!("ask: \"{}\" needs \"text -> label\"", part.trim()))?;
                let (text, label) = (text.trim(), label.trim());
                if text.is_empty() || !name_ok(label) {
                    return Err(format!("ask: \"{}\" needs \"text -> label\"", part.trim()));
                }
                options.push((text.to_string(), label.to_string()));
            }
            if options.is_empty() {
                return Err("ask: nothing to choose from".into());
            }
            Action::Ask(options)
        }
        "label" | "goto" => {
            need(1, "needs a name")?;
            if !name_ok(words[0]) {
                return Err(format!("{verb}: \"{}\" isn't a name (lowercase, digits, _)", words[0]));
            }
            if verb == "label" { Action::Label(words[0].into()) } else { Action::Goto(words[0].into()) }
        }
        "if" | "unless" => {
            if words.len() != 3 || words[1] != "goto" || !name_ok(words[0]) || !name_ok(words[2]) {
                return Err(format!("{verb}: \"{verb} flag goto label\""));
            }
            Action::If { flag: words[0].into(), label: words[2].into(), set: verb == "if" }
        }
        "set" | "add" => {
            need(1, "needs a flag")?;
            if !name_ok(words[0]) {
                return Err(format!("{verb}: \"{}\" isn't a flag name", words[0]));
            }
            let value = match words.get(1) {
                Some(v) => v.parse().map_err(|_| format!("{verb}: \"{v}\" isn't a whole number"))?,
                None if verb == "set" => 1,
                None => return Err("add: needs a value".into()),
            };
            if verb == "set" { Action::Set { flag: words[0].into(), value } } else { Action::Add { flag: words[0].into(), value } }
        }
        "clear" => {
            need(1, "needs a flag")?;
            Action::Clear(words[0].into())
        }
        "move" => {
            need(2, "needs who and at least one cell")?;
            let cells = words[1..].iter().map(|w| cell(w)).collect::<Result<Vec<_>, _>>().map_err(|e| format!("move: {e}"))?;
            Action::Move { who: who(words[0])?, cells }
        }
        "face" => {
            need(2, "needs who and a direction (n, s, e, w, player or an actor)")?;
            if !matches!(words[1], "n" | "s" | "e" | "w" | "player") && !name_ok(words[1]) {
                return Err(format!("face: \"{}\" isn't n, s, e, w, player or an actor", words[1]));
            }
            Action::Face { who: who(words[0])?, to: words[1].into() }
        }
        "wait" => {
            need(1, "needs seconds")?;
            let s: f64 = words[0].parse().map_err(|_| format!("wait: \"{}\" isn't a number of seconds", words[0]))?;
            if !(0.0..=120.0).contains(&s) {
                return Err("wait: 0 to 120 seconds".into());
            }
            Action::Wait(s)
        }
        "show" | "hide" => {
            need(1, "needs an actor")?;
            if verb == "show" { Action::Show(who(words[0])?) } else { Action::Hide(who(words[0])?) }
        }
        "sound" => {
            need(1, "needs a file under assets/")?;
            let volume = match words.get(1) {
                Some(v) => v.parse().map_err(|_| format!("sound: \"{v}\" isn't a volume"))?,
                None => 1.0,
            };
            Action::Sound { file: words[0].into(), volume }
        }
        "camera" => {
            need(1, "needs a cell (x,z), an actor or player")?;
            Action::Camera(match cell(words[0]) {
                Ok(c) => Target::Cell(c),
                Err(_) => Target::Who(who(words[0])?),
            })
        }
        "warp" => {
            need(2, "needs a map and a cell (x,z)")?;
            Action::Warp { map: words[0].into(), at: cell(words[1]).map_err(|e| format!("warp: {e}"))? }
        }
        "battle" => {
            need(1, "needs an encounter")?;
            Action::Battle(words[0].into())
        }
        "give" => {
            need(1, "needs an item")?;
            if !name_ok(words[0]) {
                return Err(format!("give: \"{}\" isn't an item's id", words[0]));
            }
            Action::Give(words[0].into())
        }
        "end" => Action::End,
        _ => return Err(format!("\"{verb}\": not an action (say, ask, label, goto, if, unless, set, add, clear, move, face, wait, show, hide, sound, camera, warp, battle, give, end)")),
    };
    Ok(Some(action))
}

/// A whole script; errors say which line.
pub fn parse(lines: &[String]) -> Result<Vec<Action>, Vec<String>> {
    let mut out = vec![];
    let mut errors = vec![];
    for (i, l) in lines.iter().enumerate() {
        match parse_line(l) {
            Ok(Some(a)) => out.push(a),
            Ok(None) => {}
            Err(e) => errors.push(format!("line {}: {e}", i + 1)),
        }
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

/// What the script refers to that must exist.
#[derive(Default)]
pub struct Known<'a> {
    /// None: don't check (a template's scripts name the actors of where it's put down)
    pub actors: Option<&'a [String]>,
    /// None: don't check
    pub maps: Option<&'a [String]>,
    pub encounters: Option<&'a [String]>,
}

/// What's wrong with a script: lines that don't parse, labels that aren't there, actors
/// the map doesn't have, maps and encounters that don't exist.
pub fn problems(lines: &[String], known: &Known) -> Vec<String> {
    let actions = match parse(lines) {
        Ok(a) => a,
        Err(e) => return e,
    };
    let mut out = vec![];
    let labels: Vec<&str> = actions.iter().filter_map(|a| if let Action::Label(l) = a { Some(l.as_str()) } else { None }).collect();
    let label = |l: &str, out: &mut Vec<String>| {
        if !labels.contains(&l) {
            out.push(format!("no label \"{l}\""));
        }
    };
    let actor = |w: &str, out: &mut Vec<String>| {
        if w != "player" && known.actors.map(|all| !all.iter().any(|a| a == w)).unwrap_or(false) {
            out.push(format!("no actor \"{w}\" on the map"));
        }
    };
    for a in &actions {
        match a {
            Action::Ask(options) => options.iter().for_each(|(_, l)| label(l, &mut out)),
            Action::Goto(l) | Action::If { label: l, .. } => label(l, &mut out),
            Action::Move { who, .. } | Action::Show(who) | Action::Hide(who) => actor(who, &mut out),
            Action::Face { who, to } => {
                actor(who, &mut out);
                if !matches!(to.as_str(), "n" | "s" | "e" | "w") {
                    actor(to, &mut out);
                }
            }
            Action::Camera(Target::Who(who)) => actor(who, &mut out),
            Action::Warp { map, .. } => {
                if let Some(maps) = known.maps {
                    if !maps.iter().any(|m| m == map) {
                        out.push(format!("no map \"{map}\""));
                    }
                }
            }
            Action::Battle(e) => {
                if let Some(all) = known.encounters {
                    if !all.iter().any(|x| x == e) {
                        out.push(format!("no encounter \"{e}\""));
                    }
                }
            }
            _ => {}
        }
    }
    out.dedup();
    out
}

/// A script moved along with what it belongs to: the cells it names (`move`, `camera`)
/// shifted by `by`, and actors renamed (`renames`: old id, new id). Lines it doesn't change
/// stay as they were written.
pub fn shift(lines: &[String], by: [i32; 2], renames: &[(String, String)]) -> Vec<String> {
    let rename = |w: &str| renames.iter().find(|(a, _)| a == w).map(|(_, b)| b.clone()).unwrap_or_else(|| w.to_string());
    let cell = |c: &[i32; 2]| format!("{},{}", c[0] + by[0], c[1] + by[1]);
    lines
        .iter()
        .map(|line| {
            let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            match parse_line(line) {
                Ok(Some(Action::Move { who, cells })) => format!("{indent}move {} {}", rename(&who), cells.iter().map(cell).collect::<Vec<_>>().join(" ")),
                Ok(Some(Action::Face { who, to })) => format!("{indent}face {} {}", rename(&who), if matches!(to.as_str(), "n" | "s" | "e" | "w") { to } else { rename(&to) }),
                Ok(Some(Action::Show(who))) => format!("{indent}show {}", rename(&who)),
                Ok(Some(Action::Hide(who))) => format!("{indent}hide {}", rename(&who)),
                Ok(Some(Action::Camera(Target::Cell(c)))) => format!("{indent}camera {}", cell(&c)),
                Ok(Some(Action::Camera(Target::Who(who)))) => format!("{indent}camera {}", rename(&who)),
                _ => line.clone(),
            }
        })
        .collect()
}

/// The sound files a script plays.
pub fn sound_files(lines: &[String]) -> Vec<String> {
    parse(lines).unwrap_or_default().into_iter().filter_map(|a| if let Action::Sound { file, .. } = a { Some(file) } else { None }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn a_script_reads_like_directions() {
        let script = lines("# a greeting\nsay Bo: Nice day for a walk.\nask Sure is -> yes | Not really -> no\nlabel yes\nset met_bo\nmove bo 12,8 12,11\nface bo player\nwait 0.5\ncamera 12,8\nsound sounds/hum.wav 0.4\nunless met_bo goto no\nlabel no\nend");
        let actions = parse(&script).unwrap();
        assert_eq!(actions[0], Action::Say { who: "Bo".into(), text: "Nice day for a walk.".into() });
        assert_eq!(actions[1], Action::Ask(vec![("Sure is".into(), "yes".into()), ("Not really".into(), "no".into())]));
        assert_eq!(actions[3], Action::Set { flag: "met_bo".into(), value: 1 });
        assert_eq!(actions[4], Action::Move { who: "bo".into(), cells: vec![[12, 8], [12, 11]] });
        assert_eq!(actions[7], Action::Camera(Target::Cell([12, 8])));
        assert_eq!(actions[8], Action::Sound { file: "sounds/hum.wav".into(), volume: 0.4 });
        assert_eq!(actions[9], Action::If { flag: "met_bo".into(), label: "no".into(), set: false });
        assert_eq!(problems(&script, &Known { actors: Some(&["bo".into()]), ..Default::default() }), Vec::<String>::new());
        assert_eq!(sound_files(&script), vec!["sounds/hum.wav".to_string()]);
    }

    #[test]
    fn a_script_moves_along_with_its_place() {
        let script = lines("say Bo: Follow me.\nmove bo 12,8 12,11\n  face bo pip\nface player n\ncamera 3,4\ncamera pip\nhide bo\nwarp meadow 5,5\n# move bo 1,1");
        let moved = shift(&script, [10, -2], &[("bo".into(), "bo_2".into()), ("pip".into(), "pip_2".into())]);
        assert_eq!(moved, lines("say Bo: Follow me.\nmove bo_2 22,6 22,9\n  face bo_2 pip_2\nface player n\ncamera 13,2\ncamera pip_2\nhide bo_2\nwarp meadow 5,5\n# move bo 1,1"));
    }

    #[test]
    fn mistakes_are_named() {
        let bad = lines("say\nsing Bo: la\nmove bo twelve\nask Yes - yes\ngoto nowhere\nwait 500");
        let errors = parse(&bad).unwrap_err();
        assert_eq!(errors.len(), 5, "{errors:?}");
        assert!(errors[0].starts_with("line 1: say"));
        assert!(errors[1].contains("not an action"));
        // give: an item's id, nothing else
        assert_eq!(parse_line("give potion").unwrap(), Some(Action::Give("potion".into())));
        assert!(parse_line("give").is_err() && parse_line("give Pot ion").is_err());
        assert!(errors[2].contains("isn't a cell"));
        assert!(errors[3].contains("text -> label"));
        assert!(errors[4].contains("0 to 120"));
        let dangling = lines("goto nowhere\nmove ghost 1,1\nwarp moon 1,1\nbattle big_one");
        let known = Known { actors: Some(&["bo".into()]), maps: Some(&["meadow".into()]), encounters: Some(&[]) };
        let p = problems(&dangling, &known);
        assert_eq!(p, vec!["no label \"nowhere\"", "no actor \"ghost\" on the map", "no map \"moon\"", "no encounter \"big_one\""]);
    }
}
