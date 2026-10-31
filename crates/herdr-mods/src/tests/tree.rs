use super::*;
use herdr_commands::Command;

#[test]
fn a_tree_reads_into_typed_nodes() {
    let node = view(
        "ui.row { gap = \"small\", align = \"center\",\n\
           ui.badge { text = \"3\", tone = \"attention\" },\n\
           ui.button { label = \"New window\", command = \"new_window\" },\n\
           ui.button { file = \"x.svg\", msg = { open = true } },\n\
           \"plain\",\n\
         }",
    )
    .unwrap();
    let Node::Row(row) = node else {
        panic!("expected a row, got {node:?}");
    };
    assert_eq!((row.gap, row.align), (Gap::Small, Align::Center));
    assert_eq!(row.children.len(), 4);
    assert_eq!(
        row.children[0],
        Node::Badge(Badge {
            text: "3".to_owned(),
            tone: Tone::Attention
        })
    );
    let Node::Button(button) = &row.children[1] else {
        panic!("expected a button");
    };
    assert_eq!(button.action, Action::Command(Command::NewWindow));
    let Node::Button(button) = &row.children[2] else {
        panic!("expected a button");
    };
    assert_eq!(button.icon, Some(IconSource::File("x.svg".to_owned())));
    let open = [("open".to_owned(), Data::Bool(true))]
        .into_iter()
        .collect();
    assert_eq!(button.action, Action::Message(Data::Map(open)));
    assert_eq!(row.children[3], text("plain"));
    assert_eq!(Node::Row(row).count(), 5);
}

#[test]
fn ui_text_takes_a_bare_string() {
    assert_eq!(view("ui.text(\"hi\")").unwrap(), text("hi"));
}

#[test]
fn every_element_has_a_builder() {
    let checks = Element::NAMES
        .iter()
        .map(|name| format!("assert(ui.{name} ~= nil, \"{name}\")\n"))
        .collect::<String>();
    load(&format!(
        "local ui = require(\"@herdr/ui\")\n{checks}{}",
        view_mod("ui.spacer()")
    ))
    .unwrap();
}

#[test]
fn unknown_keys_are_reported_where_they_are() {
    let (at, problem) =
        tree_problem("ui.column { ui.row { ui.button { lable = \"x\", msg = 1 } } }");
    assert_eq!(at, "column[1].row[1].button");
    assert_eq!(problem, Problem::Unknown("lable".to_owned()));
}

#[test]
fn unknown_elements_and_values_are_refused() {
    assert_eq!(
        tree_problem("{ type = \"canvas\" }").1,
        Problem::Element("canvas".to_owned())
    );
    assert_eq!(
        tree_problem("ui.text { text = \"x\", tone = \"red\" }"),
        ("text.tone".to_owned(), Problem::OneOf(Tone::NAMES))
    );
    assert_eq!(
        tree_problem("ui.button { label = \"x\", command = \"rm_rf\" }"),
        (
            "button.command".to_owned(),
            Problem::Command("rm_rf".to_owned())
        )
    );
    assert_eq!(
        tree_problem("ui.button { label = \"x\" }"),
        ("button.command".to_owned(), Problem::Missing)
    );
    assert_eq!(tree_problem("ui.meter { value = 1.5 }").0, "meter.value");
    assert_eq!(
        tree_problem("ui.link { label = \"x\", url = \"http://a\" }").0,
        "link.url"
    );
    assert_eq!(tree_problem("ui.icon { name = \"../x\" }").0, "icon.name");
    assert_eq!(
        tree_problem("42").1,
        Problem::Expected {
            expected: "an element from `ui`, or a string"
        }
    );
}

#[test]
fn leaves_take_no_children() {
    let (at, _) = tree_problem("ui.badge { text = \"x\", ui.spacer() }");
    assert_eq!(at, "badge");
    let (_, problem) = tree_problem("ui.tooltip { text = \"x\" }");
    assert_eq!(
        problem,
        Problem::Expected {
            expected: "exactly one child"
        }
    );
}

#[test]
fn functions_cannot_hide_in_messages() {
    let (at, _) = tree_problem("ui.button { label = \"x\", msg = { go = function() end } }");
    assert_eq!(at, "button.msg.go");
}

#[test]
fn trees_are_bounded() {
    let (_, problem) = tree_problem(
        "(function() local r = ui.column {} for i = 1, 500 do r[i] = ui.spacer() end return r end)()",
    );
    assert_eq!(problem, Problem::Nodes { limit: 200 });
    let (_, problem) = tree_problem(
        "(function() local n = ui.spacer() for i = 1, 20 do n = ui.row { n } end return n end)()",
    );
    assert_eq!(problem, Problem::Depth { limit: 16 });
    let (_, problem) = tree_problem("string.rep(\"x\", 2000)");
    assert_eq!(problem, Problem::TooLong { limit: 1024 });
}
