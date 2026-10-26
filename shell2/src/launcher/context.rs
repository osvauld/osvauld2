use super::{Command, navigation, session::Inventory, spaces_api};
use crate::{Msg, item::ItemsScreenMsg};
use app_host::{LuaMsg, host::VmSetup};
use std::{cell::RefCell, rc::Rc};
use vault::WorkspaceMeta;

pub(crate) type Spaces = Rc<RefCell<List>>;

#[derive(Default)]
pub(crate) struct List {
    pub rows: Vec<WorkspaceMeta>,
    pub creation: super::creation::Creation,
}

#[derive(Clone)]
pub(crate) enum Context {
    Workspace(Inventory),
    Spaces(Spaces),
}

impl Context {
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Workspace(a), Self::Workspace(b)) => Rc::ptr_eq(a, b),
            (Self::Spaces(a), Self::Spaces(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    pub fn entry(&self) -> &'static str {
        match self {
            Self::Workspace(_) => "main.lua",
            Self::Spaces(_) => "spaces_view.lua",
        }
    }

    pub fn identity(&self, instance: u64) -> Rc<dyn Fn(LuaMsg) -> Msg> {
        match self {
            Self::Workspace(inventory) => {
                let id = inventory.borrow().workspace.id.clone();
                Rc::new(move |msg| Msg::Items(ItemsScreenMsg::Launcher(id.clone(), instance, msg)))
            }
            Self::Spaces(_) => Rc::new(move |msg| Msg::Launcher(instance, msg)),
        }
    }

    pub fn setup(&self, commands: Rc<RefCell<Vec<Command>>>) -> VmSetup {
        match self {
            Self::Workspace(inventory) => {
                let queue = commands.clone();
                let inventory_setup = crate::shell_api::setup(
                    inventory.clone(),
                    Rc::new(move |item| queue.borrow_mut().push(Command::OpenItem(item))),
                );
                let navigation =
                    navigation::setup(inventory.borrow().workspace.id.clone(), commands);
                Rc::new(move |vm, context| {
                    inventory_setup(vm, context.clone())?;
                    navigation(vm, context)
                })
            }
            Self::Spaces(spaces) => spaces_api::setup(spaces.clone(), commands),
        }
    }
}
