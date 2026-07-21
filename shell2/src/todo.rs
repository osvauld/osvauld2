use crate::theme;
use runtime::{col, row, text, text_input, DragEvent, DragPhase, El};

pub enum Event {
    BackRequested,
}
struct Todo {
    id: u64,
    text: String,
    done: bool,
}
pub struct TodoScreen {
    items: Vec<Todo>,
    next_id: u64,
    editing: Option<u64>,
    dragging: Option<(u64, usize)>,
}
#[derive(Clone)]
pub enum Msg {
    New,
    Update(String, u64),
    Delete(u64),
    Done(u64),
    Back,
    ToggleEdit(u64),
    Reorder(u64, DragEvent),
}

impl TodoScreen {
    pub fn new() -> Self {
        Self {
            items: vec![],
            next_id: 0,
            editing: None,
            dragging: None,
        }
    }
    pub fn update(&mut self, msg: Msg) -> Option<Event> {
        match msg {
            Msg::New => {
                let todo = Todo {
                    id: self.next_id,
                    text: String::new(),
                    done: false,
                };
                self.next_id += 1;
                self.editing = Some(todo.id);
                self.items.push(todo);
                None
            }
            Msg::Update(text, id) => {
                let item = self.items.iter_mut().find(|t| t.id == id);
                if let Some(item) = item {
                    item.text = text;
                }
                None
            }
            Msg::Delete(id) => {
                self.items.retain(|t| t.id != id);
                None
            }

            Msg::Done(id) => {
                let item = self.items.iter_mut().find(|t| t.id == id);
                if let Some(item) = item {
                    item.done = !item.done;
                }
                None
            }
            Msg::Back => Some(Event::BackRequested),
            Msg::ToggleEdit(id) => {
                if let Some(editing) = self.editing {
                    if editing == id {
                        self.editing = None;
                        return None;
                    }
                }
                self.editing = Some(id);
                None
            }
            Msg::Reorder(id, event) => match event.phase {
                DragPhase::Start => {
                    let home = self.items.iter().position(|t| t.id == id).unwrap_or(0);
                    self.dragging = Some((id, home));
                    None
                }
                DragPhase::Move => {
                    if let Some((id, home)) = self.dragging {
                        let slots = (event.delta.1 / 56.0).round() as i32;
                        let desired =
                            (home as i32 + slots).clamp(0, self.items.len() as i32 - 1) as usize;
                        if let Some(cur) = self.items.iter().position(|t| t.id == id) {
                            if cur != desired {
                                let item = self.items.remove(cur);
                                self.items.insert(desired, item);
                            }
                        }
                    }
                    None
                }
                DragPhase::End => {
                    self.dragging = None;
                    None
                }
            },
        }
    }

    pub fn view(&self) -> El<Msg> {
        let add_todo = row()
            .h(36.0)
            .px(14.0)
            .center()
            .radius(6.0)
            .fill(theme::accent())
            .hover_fill(theme::accent_press())
            .on_click(Msg::New)
            .child(text("Add todo"));
        let mut todos = Vec::new();

        for r in &self.items {
            let id = format!("todo:{}", r.id);
            let todo_id = r.id;
            let grip = col()
                .size(24.0, 48.0)
                .center()
                .on_drag(format!("todorow:{}", r.id), move |e| {
                    Msg::Reorder(todo_id, e)
                })
                .child(text("::").color(theme::fg_4()));
            let editing = self.editing == Some(r.id);
            let t_row = if editing {
                text_input(&r.text, id, move |s| Msg::Update(s, todo_id))
                    .grow()
                    .h(36.0)
                    .autofocus()
                    .on_enter(Msg::ToggleEdit(todo_id))
                    .on_esc(Msg::ToggleEdit(todo_id))
                    .px(12.0)
                    .font_size(15.0)
                    .color(if r.done { theme::fg_4() } else { theme::fg_1() })
                    .stroke(1.0, theme::bd_1())
            } else {
                text(&r.text)
                    .grow()
                    .h(36.0)
                    .px(12.0)
                    .font_size(15.0)
                    .color(if r.done { theme::fg_4() } else { theme::fg_1() })
                    .stroke(1.0, theme::bd_1())
            };
            let checkbox = checkbox(r.done, Msg::Done(r.id));
            let del = col()
                .size(24.0, 24.0)
                .center()
                .radius(4.0)
                .hover_fill(theme::fg_2())
                .on_click(Msg::Delete(r.id))
                .child(text("x").font_size(14.0).color(theme::fg_4()));
            let edit = col()
                .size(24.0, 24.0)
                .center()
                .radius(4.0)
                .hover_fill(theme::fg_2())
                .on_click(Msg::ToggleEdit(r.id))
                .child(text("E").font_size(14.0))
                .fill(if editing {
                    theme::fg_4()
                } else {
                    theme::fg_2()
                });
            let k = row()
                .h(48.0)
                .w(400.0)
                .px(14.0)
                .stroke(1.0, theme::accent())
                .center()
                .child(grip)
                .child(t_row)
                .child(del)
                .child(edit)
                .child(checkbox);
            todos.push(k);
        }

        let panel = col()
            .scroll_y("todo:list")
            .h(400.0)
            .gap(8.0)
            .children(todos);
        let back = col()
            .w(40.0)
            .h(20.0)
            .center()
            .on_click(Msg::Back)
            .child(text("back"))
            .fill(theme::accent_press());
        col()
            .full()
            .center()
            .gap(8.0)
            .child(panel)
            .child(add_todo)
            .child(back)
    }
}
pub fn checkbox<M>(checked: bool, msg: M) -> El<M> {
    let mut b = col().size(18.0, 18.0).radius(4.0).center().on_click(msg);
    if checked {
        b = b
            .fill(theme::accent())
            .child(text("✓").font_size(12.0).color(theme::fg_1()))
            .hover_stroke(1.0, theme::accent());
    } else {
        b = b
            .stroke(1.0, theme::bd_1())
            .hover_stroke(1.0, theme::accent())
    }
    b
}
