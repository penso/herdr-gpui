//! Reads a `view`'s Lua tables into [`Node`]s. Unknown elements and keys are
//! refused rather than ignored, so a typo is reported where it is.

use super::*;
use crate::{Problem, data};
use herdr_commands::COMMANDS;
use mlua::Value;
use std::collections::BTreeMap;

const MAX_DEPTH: usize = 16;
const MAX_TEXT: usize = 1024;
const MAX_LABEL: usize = 120;
const MAX_URL: usize = 2048;

type Found<T> = Result<T, (String, Problem)>;

/// Reads a whole tree of at most `nodes` elements.
pub(crate) fn read(value: &Value, nodes: usize) -> Found<Node> {
    let mut reader = Reader {
        nodes: 0,
        limit: nodes,
    };
    reader.node(value, "", 0)
}

struct Reader {
    nodes: usize,
    limit: usize,
}

impl Reader {
    fn node(&mut self, value: &Value, parent: &str, depth: usize) -> Found<Node> {
        self.nodes += 1;
        if self.nodes > self.limit {
            let limit = self.limit;
            return Err((parent.to_owned(), Problem::Nodes { limit }));
        }
        if depth == MAX_DEPTH {
            let limit = MAX_DEPTH;
            return Err((parent.to_owned(), Problem::Depth { limit }));
        }
        let table = match value {
            // A bare string in a container is a text element.
            Value::String(text) => {
                return Ok(Node::Text(Text {
                    text: data::string(text, parent, MAX_TEXT)?,
                    tone: Tone::Normal,
                    size: Size::Normal,
                    weight: Weight::Normal,
                    truncate: false,
                }));
            }
            Value::Table(table) => table,
            _ => {
                return Err((
                    parent.to_owned(),
                    Problem::Expected {
                        expected: "an element from `ui`, or a string",
                    },
                ));
            }
        };
        let mut props = Props::new(table, parent)?;
        let kind = props
            .text("type", MAX_LABEL)?
            .ok_or_else(|| (props.at("type"), Problem::Missing))?;
        let element =
            Element::parse(&kind).ok_or_else(|| (props.at("type"), Problem::Element(kind)))?;
        props.name(element.name());
        if let Some(key) = props
            .keys
            .keys()
            .find(|key| !element.keys().contains(&key.as_str()))
        {
            return Err((props.here(), Problem::Unknown(key.clone())));
        }
        let node = match element {
            Element::Row | Element::Column => {
                let container = Container {
                    gap: props
                        .named("gap", Gap::parse, Gap::NAMES)?
                        .unwrap_or(Gap::Normal),
                    align: props
                        .named("align", Align::parse, Align::NAMES)?
                        .unwrap_or(Align::Stretch),
                    children: Vec::new(),
                };
                let children = self.children(&mut props, depth)?;
                let container = Container {
                    children,
                    ..container
                };
                match element {
                    Element::Row => Node::Row(container),
                    _ => Node::Column(container),
                }
            }
            Element::List => Node::List(List {
                items: self.children(&mut props, depth)?,
            }),
            Element::Text => Node::Text(Text {
                text: props.required_text("text", MAX_TEXT)?,
                tone: props.tone()?,
                size: props
                    .named("size", Size::parse, Size::NAMES)?
                    .unwrap_or(Size::Normal),
                weight: props
                    .named("weight", Weight::parse, Weight::NAMES)?
                    .unwrap_or(Weight::Normal),
                truncate: props.bool("truncate")?.unwrap_or(false),
            }),
            Element::Icon => Node::Icon(Icon {
                source: props
                    .icon()?
                    .ok_or_else(|| (props.at("name"), Problem::Missing))?,
                tone: props.tone()?,
            }),
            Element::Badge => Node::Badge(Badge {
                text: props.required_text("text", MAX_LABEL)?,
                tone: props.tone()?,
            }),
            Element::Button => {
                let label = props.text("label", MAX_LABEL)?;
                let icon = props.icon()?;
                if label.is_none() && icon.is_none() {
                    return Err((props.at("label"), Problem::Missing));
                }
                Node::Button(Button {
                    label,
                    icon,
                    tone: props.tone()?,
                    action: props.action()?,
                    disabled: props.bool("disabled")?.unwrap_or(false),
                })
            }
            Element::Progress => Node::Progress(Progress {
                value: props.fraction("value")?,
                tone: props.tone()?,
            }),
            Element::Meter => Node::Meter(Meter {
                value: props
                    .fraction("value")?
                    .ok_or_else(|| (props.at("value"), Problem::Missing))?,
                label: props.text("label", MAX_LABEL)?,
                tone: props.tone()?,
            }),
            Element::Link => {
                let url = props.required_text("url", MAX_URL)?;
                if !url.starts_with("https://") {
                    return Err((
                        props.at("url"),
                        Problem::Expected {
                            expected: "an https:// URL",
                        },
                    ));
                }
                Node::Link(Link {
                    label: props.required_text("label", MAX_LABEL)?,
                    url,
                })
            }
            Element::Toggle => Node::Toggle(Toggle {
                label: props.required_text("label", MAX_LABEL)?,
                on: props.bool("on")?.unwrap_or(false),
                msg: props
                    .data("msg")?
                    .ok_or_else(|| (props.at("msg"), Problem::Missing))?,
            }),
            Element::Tooltip => {
                let text = props.required_text("text", MAX_TEXT)?;
                let mut children = self.children(&mut props, depth)?;
                if children.len() != 1 {
                    return Err((
                        props.here(),
                        Problem::Expected {
                            expected: "exactly one child",
                        },
                    ));
                }
                let child = Box::new(children.remove(0));
                Node::Tooltip(Tooltip { text, child })
            }
            Element::Spacer => Node::Spacer,
            Element::Divider => Node::Divider,
        };
        props.finish()?;
        Ok(node)
    }

    fn children(&mut self, props: &mut Props, depth: usize) -> Found<Vec<Node>> {
        let values = std::mem::take(&mut props.children);
        values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                self.node(
                    value,
                    &format!("{}[{}]", props.here(), index + 1),
                    depth + 1,
                )
            })
            .collect()
    }
}

/// One element's keys, taken one by one; what is left over is refused.
struct Props {
    path: String,
    keys: BTreeMap<String, Value>,
    children: Vec<Value>,
}

impl Props {
    fn new(table: &mlua::Table, parent: &str) -> Found<Self> {
        let length = table.raw_len();
        let shape = || {
            (
                parent.to_owned(),
                Problem::Expected {
                    expected: "an element: named keys plus a list of children",
                },
            )
        };
        let mut keys = BTreeMap::new();
        for pair in table.pairs::<Value, Value>() {
            let (key, value) = pair.map_err(|_| shape())?;
            match key {
                Value::Integer(index) if index >= 1 && index as usize <= length => {}
                Value::String(key) => {
                    keys.insert(data::string(&key, parent, MAX_LABEL)?, value);
                }
                _ => return Err(shape()),
            }
        }
        let children = (1..=length)
            .map(|index| table.raw_get::<Value>(index).map_err(|_| shape()))
            .collect::<Found<_>>()?;
        Ok(Self {
            path: parent.to_owned(),
            keys,
            children,
        })
    }

    /// Names this element in the paths of its problems.
    fn name(&mut self, element: &str) {
        self.path = if self.path.is_empty() {
            element.to_owned()
        } else {
            format!("{}.{element}", self.path)
        };
    }

    fn here(&self) -> String {
        self.path.clone()
    }

    fn at(&self, key: &str) -> String {
        if self.path.is_empty() {
            key.to_owned()
        } else {
            format!("{}.{key}", self.path)
        }
    }

    fn take(&mut self, key: &str) -> Option<Value> {
        self.keys.remove(key).filter(|value| !value.is_nil())
    }

    fn expected(&self, key: &str, expected: &'static str) -> (String, Problem) {
        (self.at(key), Problem::Expected { expected })
    }

    fn text(&mut self, key: &str, limit: usize) -> Found<Option<String>> {
        match self.take(key) {
            None => Ok(None),
            Some(Value::String(text)) => {
                let text = data::string(&text, &self.at(key), limit)?;
                if text.chars().any(|c| c.is_control() && c != '\n') {
                    return Err((self.at(key), Problem::Control));
                }
                Ok(Some(text))
            }
            Some(_) => Err(self.expected(key, "a string")),
        }
    }

    fn required_text(&mut self, key: &str, limit: usize) -> Found<String> {
        self.text(key, limit)?
            .ok_or_else(|| (self.at(key), Problem::Missing))
    }

    fn named<T>(
        &mut self,
        key: &str,
        parse: fn(&str) -> Option<T>,
        names: &'static [&'static str],
    ) -> Found<Option<T>> {
        let Some(text) = self.text(key, MAX_LABEL)? else {
            return Ok(None);
        };
        parse(&text)
            .map(Some)
            .ok_or_else(|| (self.at(key), Problem::OneOf(names)))
    }

    fn tone(&mut self) -> Found<Tone> {
        Ok(self
            .named("tone", Tone::parse, Tone::NAMES)?
            .unwrap_or(Tone::Normal))
    }

    fn bool(&mut self, key: &str) -> Found<Option<bool>> {
        match self.take(key) {
            None => Ok(None),
            Some(Value::Boolean(value)) => Ok(Some(value)),
            Some(_) => Err(self.expected(key, "true or false")),
        }
    }

    fn fraction(&mut self, key: &str) -> Found<Option<f64>> {
        let value = match self.take(key) {
            None => return Ok(None),
            Some(Value::Integer(value)) => value as f64,
            Some(Value::Number(value)) => value,
            Some(_) => return Err(self.expected(key, "a number from 0 to 1")),
        };
        if !(0. ..=1.).contains(&value) {
            return Err(self.expected(key, "a number from 0 to 1"));
        }
        Ok(Some(value))
    }

    fn data(&mut self, key: &str) -> Found<Option<Data>> {
        let Some(value) = self.take(key) else {
            return Ok(None);
        };
        let at = self.at(key);
        Data::read(&value)
            .map(Some)
            .map_err(|(inner, problem)| (format!("{at}{inner}"), problem))
    }

    /// `name` for an app icon, or `file` for an SVG in the mod's folder.
    fn icon(&mut self) -> Found<Option<IconSource>> {
        let name = self.text("name", MAX_LABEL)?;
        let file = self.text("file", MAX_LABEL)?;
        match (name, file) {
            (Some(_), Some(_)) => Err(self.expected("file", "absent when `name` is given")),
            (Some(name), None) => crate::manifest::identifier(&name)
                .map(|()| Some(IconSource::App(name)))
                .map_err(|problem| (self.at("name"), problem)),
            (None, Some(file)) => crate::manifest::icon_file(&file)
                .map(|()| Some(IconSource::File(file)))
                .map_err(|problem| (self.at("file"), problem)),
            (None, None) => Ok(None),
        }
    }

    /// `command` names a catalog command; `msg` is sent to `update`.
    fn action(&mut self) -> Found<Action> {
        let command = self.text("command", MAX_LABEL)?;
        let msg = self.data("msg")?;
        match (command, msg) {
            (Some(_), Some(_)) => Err(self.expected("msg", "absent when `command` is given")),
            (Some(name), None) => command_named(&name)
                .map(Action::Command)
                .ok_or_else(|| (self.at("command"), Problem::Command(name))),
            (None, Some(msg)) => Ok(Action::Message(msg)),
            (None, None) => Err((self.at("command"), Problem::Missing)),
        }
    }

    fn finish(self) -> Found<()> {
        if let Some(key) = self.keys.into_keys().next() {
            let at = self.path.clone();
            return Err((at, Problem::Unknown(key)));
        }
        if !self.children.is_empty() {
            return Err((
                self.path,
                Problem::Expected {
                    expected: "no children",
                },
            ));
        }
        Ok(())
    }
}

pub(crate) fn command_named(name: &str) -> Option<Command> {
    COMMANDS
        .iter()
        .find(|info| info.name == name)
        .map(|info| info.command)
}
