//! Reader toolbars shed one action at a time, using the real header's sizes.
//! A breakpoint bin owns the width so hidden buttons cannot raise its minimum.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::config::{ReaderToolbar, ToolbarItem};

#[cfg(test)]
mod tests;

pub(crate) struct ToolbarFit {
    bin: gtk::glib::WeakRef<adw::BreakpointBin>,
    header: adw::HeaderBar,
    buttons: Vec<(ToolbarItem, gtk::Widget)>,
    overflow: gtk::Button,
    spinner: gtk::Widget,
    breakpoints: Vec<adw::Breakpoint>,
    order: RefCell<Vec<ToolbarItem>>,
    queued: Cell<bool>,
}

impl ToolbarFit {
    pub(crate) fn new(
        bin: &adw::BreakpointBin,
        header: &adw::HeaderBar,
        buttons: &[(ToolbarItem, gtk::Widget)],
        overflow: &gtk::Button,
        spinner: &impl IsA<gtk::Widget>,
        changed: impl Fn(usize) + 'static,
    ) -> Rc<Self> {
        let breakpoints: Vec<_> = ToolbarItem::ALL.iter().map(|_| breakpoint(0)).collect();
        // The last matching breakpoint wins: the narrowest belongs last.
        for bp in breakpoints.iter().rev() {
            bin.add_breakpoint(bp.clone());
        }
        let bps = breakpoints.clone();
        bin.connect_current_breakpoint_notify(move |bin| {
            let current = bin.current_breakpoint();
            let slots = bps
                .iter()
                .position(|bp| Some(bp) == current.as_ref())
                .unwrap_or(usize::MAX);
            changed(slots);
        });
        let fit = Rc::new(Self {
            bin: bin.downgrade(),
            header: header.clone(),
            buttons: buttons.to_vec(),
            overflow: overflow.clone(),
            spinner: spinner.as_ref().clone(),
            breakpoints,
            order: RefCell::new(Vec::new()),
            queued: Cell::new(false),
        });
        // Mapping includes reparenting into phone navigation, where Adwaita
        // adds Back. Wait until that and the mobile CSS have taken effect.
        let weak = Rc::downgrade(&fit);
        header.connect_map(move |_| {
            if let Some(fit) = weak.upgrade() {
                fit.refresh();
            }
        });
        for property in [
            "decoration-layout",
            "show-back-button",
            "show-start-title-buttons",
            "show-end-title-buttons",
        ] {
            let weak = Rc::downgrade(&fit);
            header.connect_notify_local(Some(property), move |_, _| {
                if let Some(fit) = weak.upgrade() {
                    fit.refresh();
                }
            });
        }
        if let Some(settings) = gtk::Settings::default() {
            for property in ["gtk-decoration-layout", "gtk-font-name", "gtk-xft-dpi"] {
                let weak = Rc::downgrade(&fit);
                settings.connect_notify_local(Some(property), move |_, _| {
                    if let Some(fit) = weak.upgrade() {
                        fit.refresh();
                    }
                });
            }
        }
        fit
    }

    pub(crate) fn set_layout(self: &Rc<Self>, layout: &ReaderToolbar) {
        self.order.replace(layout.priority());
        self.refresh();
    }

    pub(crate) fn refresh(self: &Rc<Self>) {
        if self.queued.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        gtk::glib::idle_add_local_once(move || {
            if let Some(fit) = weak.upgrade() {
                fit.queued.set(false);
                fit.measure();
            }
        });
    }

    fn measure(&self) {
        // Measure each candidate row, including native window controls,
        // Back, spacing and the spinner. Revealers otherwise report only
        // their current animated width, not the space a button will need.
        let focus = self.header.root().and_then(|root| root.focus());
        let saved: Vec<_> = self
            .buttons
            .iter()
            .map(|(_, widget)| {
                let revealer = widget.downcast_ref::<gtk::Revealer>().map(|r| {
                    let state = (r.reveals_child(), r.transition_duration());
                    r.set_transition_duration(0);
                    r.set_reveal_child(true);
                    state
                });
                // Tags and Move To have a box between the revealer and
                // button, whereas most actions are direct children.
                let mut path = Vec::new();
                let mut child = Some(widget.clone());
                while let Some(w) = child {
                    path.push((w.clone(), w.is_visible()));
                    w.set_visible(true);
                    child = if w.is::<gtk::Button>() {
                        None
                    } else {
                        w.first_child()
                    };
                }
                widget.set_visible(false);
                (widget, path, revealer)
            })
            .collect();
        let overflow = self.overflow.is_visible();
        let spinner = self.spinner.is_visible();
        self.spinner.set_visible(true);
        let order = self.order.borrow();
        for (slots, bp) in self.breakpoints.iter().enumerate() {
            let need = if let Some(item) = order.get(slots) {
                if let Some((_, widget)) = self.buttons.iter().find(|(i, _)| i == item) {
                    widget.set_visible(true);
                }
                self.overflow.set_visible(slots + 1 < order.len());
                // The natural width centres an empty title by doubling the
                // wider group. The minimum is what the row actually needs.
                self.header.measure(gtk::Orientation::Horizontal, -1).0 + 8
            } else {
                0
            };
            bp.set_condition(Some(&condition(need)));
        }
        for (widget, path, revealer) in saved {
            for (w, visible) in path.into_iter().rev() {
                w.set_visible(visible);
            }
            if let Some((shown, duration)) = revealer {
                let r = widget.downcast_ref::<gtk::Revealer>().unwrap();
                r.set_reveal_child(shown);
                r.set_transition_duration(duration);
            }
        }
        self.overflow.set_visible(overflow);
        self.spinner.set_visible(spinner);
        if let Some(focus) = focus.filter(|w| w.is_mapped()) {
            focus.grab_focus();
        }
        // A new decoration layout or text scale can change the thresholds
        // without changing the pane's allocation. Re-evaluate at that width.
        if let Some(bin) = self.bin.upgrade() {
            bin.queue_resize();
        }
    }
}

fn condition(width: i32) -> adw::BreakpointCondition {
    adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(width),
        adw::LengthUnit::Px,
    )
}

fn breakpoint(width: i32) -> adw::Breakpoint {
    adw::Breakpoint::new(condition(width))
}
