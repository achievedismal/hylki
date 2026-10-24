//! The pop-out's header toolbar: its buttons in the saved order, folding
//! into the ⋯ menu when the window is narrow, and that menu.

use super::types::{MessageWindowInput, MessageWindowOutput};
use super::MessageWindow;
use crate::config::ToolbarItem;
use crate::i18n::i18n;
use crate::ui::context_menu::{show_context_menu, MenuEntry};
use crate::ui::message_actions::MessageAction;
use crate::ui::toolbar_fit::ToolbarFit;
use adw::prelude::*;
use relm4::prelude::*;

/// The header's buttons, one per toolbar item, packed in the saved order.
pub(super) struct Toolbar {
    pub(super) header: adw::HeaderBar,
    pub(super) buttons: Vec<(ToolbarItem, gtk::Button)>,
    pub(super) overflow: gtk::Button,
    pub(super) spinner: gtk::Spinner,
    pub(super) fit: Option<std::rc::Rc<ToolbarFit>>,
}

/// One flat icon button for each toolbar item, with the main window's icons
/// and tooltips, and the ⋯ and spinner the row also needs.
pub(super) fn build_toolbar(
    bin: &adw::BreakpointBin,
    header: &adw::HeaderBar,
    sender: &ComponentSender<MessageWindow>,
) -> Toolbar {
    let button = |icon: &str, tooltip: String, msg: fn() -> MessageWindowInput| {
        let b = gtk::Button::from_icon_name(icon);
        b.set_tooltip_text(Some(&tooltip));
        b.add_css_class("flat");
        let s = sender.input_sender().clone();
        b.connect_clicked(move |_| {
            let _ = s.send(msg());
        });
        b
    };
    // The icons are the main toolbar's own (`ToolbarItem::icon`); the
    // tooltips name the shortcut where the main window's menu would.
    let buttons: Vec<_> = ToolbarItem::ALL
        .iter()
        .map(|item| {
            let (tooltip, msg): (String, fn() -> MessageWindowInput) = match item {
                ToolbarItem::Reply => (i18n("Reply"), || MessageWindowInput::Reply),
                ToolbarItem::ReplyAll => (i18n("Reply All"), || MessageWindowInput::ReplyAll),
                ToolbarItem::Forward => (i18n("Forward"), || MessageWindowInput::Forward),
                // One glyph in both states, as in the main window; the
                // starred state carries color only.
                ToolbarItem::Star => (i18n("Star"), || MessageWindowInput::ToggleStar),
                ToolbarItem::Archive => (i18n("Archive"), || MessageWindowInput::Archive),
                ToolbarItem::Delete => (i18n("Delete"), || MessageWindowInput::Delete),
                ToolbarItem::Spam => (i18n("Mark as Spam"), || MessageWindowInput::Spam),
                ToolbarItem::ReadUnread => {
                    (i18n("Mark as Unread"), || MessageWindowInput::ToggleRead)
                }
                ToolbarItem::Tags => (i18n("Tags"), || MessageWindowInput::TagMenu),
                ToolbarItem::MoveTo => (i18n("Move To…"), || MessageWindowInput::MoveTo),
                ToolbarItem::Find => (i18n("Find in message (Ctrl+F)"), || {
                    MessageWindowInput::Find
                }),
                // The preview, not the print dialog: it shows what will come
                // out and prints from there (#359).
                ToolbarItem::Print => (i18n("Print Preview (Ctrl+Shift+P)"), || {
                    MessageWindowInput::PrintPreview
                }),
            };
            (*item, button(item.icon(), tooltip, msg))
        })
        .collect();
    // Rightmost, beside the window controls, as in the main window.
    let overflow = button("view-more-horizontal-symbolic", i18n("Actions"), || {
        MessageWindowInput::OverflowMenu
    });
    overflow.set_visible(false);
    header.pack_end(&overflow);
    let spinner = gtk::Spinner::new();
    spinner.set_valign(gtk::Align::Center);
    spinner.set_tooltip_text(Some(&i18n("Downloading attachments…")));
    let s = sender.clone();
    let fit = ToolbarFit::new(
        bin,
        header,
        &buttons
            .iter()
            .map(|(i, b)| (*i, b.clone().upcast()))
            .collect::<Vec<_>>(),
        &overflow,
        &spinner,
        move |slots| s.input(MessageWindowInput::SetToolbarSlots(slots)),
    );
    Toolbar {
        header: header.clone(),
        buttons,
        overflow,
        spinner,
        fit: Some(fit),
    }
}

impl MessageWindow {
    /// Toolbars fold and attachment controls wrap; loading an attachment must
    /// not raise a phone window's minimum width to the desktop header's width.
    pub(super) fn fit_min_width(&self) {
        self.window.set_size_request(
            crate::ui::adaptive::MIN_WIDTH,
            crate::ui::adaptive::MIN_HEIGHT,
        );
    }

    pub(super) fn button(&self, item: ToolbarItem) -> Option<&gtk::Button> {
        self.bar
            .buttons
            .iter()
            .find(|(i, _)| *i == item)
            .map(|(_, b)| b)
    }

    /// Pack the buttons in the saved order, as the main window does: the
    /// left group from the start, the right group from the end (which fills
    /// right to left, so in reverse), then the attachments spinner as the
    /// innermost of the right group. Buttons on neither side stay unpacked.
    /// The fold point is measured afresh for the new row.
    pub(super) fn relayout_toolbar(&self) {
        let header = &self.bar.header;
        for (_, b) in &self.bar.buttons {
            if b.parent().is_some() {
                header.remove(b);
            }
        }
        if self.bar.spinner.parent().is_some() {
            header.remove(&self.bar.spinner);
        }
        for item in &self.toolbar.left {
            if let Some(b) = self.button(*item) {
                header.pack_start(b);
            }
        }
        for item in self.toolbar.right.iter().rev() {
            if let Some(b) = self.button(*item) {
                header.pack_end(b);
            }
        }
        header.pack_end(&self.bar.spinner);

        if let Some(fit) = &self.bar.fit {
            fit.set_layout(&self.toolbar);
        }
    }

    /// Fitting buttons and the ones that say the message's state.
    pub(super) fn sync_toolbar(&self) {
        for (item, b) in &self.bar.buttons {
            let mut visible = self.toolbar.fits(*item, self.toolbar_slots);
            if *item == ToolbarItem::Tags {
                visible &= !self.tags.is_empty();
            }
            b.set_visible(visible);
            match item {
                ToolbarItem::Star => {
                    b.set_tooltip_text(Some(&if self.msg.starred {
                        i18n("Remove Star")
                    } else {
                        i18n("Star")
                    }));
                    if self.msg.starred {
                        b.add_css_class("star-active");
                    } else {
                        b.remove_css_class("star-active");
                    }
                }
                // The icon shows the ACTION (read envelope = "mark as
                // read"), matching the menus.
                ToolbarItem::ReadUnread => {
                    b.set_icon_name(if self.msg.unread {
                        "hylki-mail-read-symbolic"
                    } else {
                        "mail-unread-symbolic"
                    });
                    b.set_tooltip_text(Some(&if self.msg.unread {
                        i18n("Mark as Read")
                    } else {
                        i18n("Mark as Unread")
                    }));
                }
                ToolbarItem::Spam => {
                    b.set_icon_name(if self.in_junk {
                        "mail-mark-notjunk-symbolic"
                    } else {
                        "mail-mark-junk-symbolic"
                    });
                    b.set_tooltip_text(Some(&if self.in_junk {
                        i18n("Not Spam")
                    } else {
                        i18n("Mark as Spam")
                    }));
                }
                _ => {}
            }
        }
        self.bar
            .overflow
            .set_visible(
                self.toolbar
                    .left
                    .iter()
                    .chain(&self.toolbar.right)
                    .any(|item| {
                        (*item != ToolbarItem::Tags || !self.tags.is_empty())
                            && !self.toolbar.fits(*item, self.toolbar_slots)
                    }),
            );
        let loading = !self.attachments_loading.is_empty();
        self.bar.spinner.set_spinning(loading);
        self.bar.spinner.set_visible(loading);
    }

    /// The tags (#71), ticked where the message carries them.
    fn tag_entries(&self, sender: &ComponentSender<Self>) -> Vec<MenuEntry> {
        let s = sender.output_sender().clone();
        let message = self.msg.clone();
        crate::ui::message_list::tag_menu_entries(&self.tags, &self.msg, move |keyword, add| {
            let _ = s.send(MessageWindowOutput::SetTag {
                message: Box::new(message.clone()),
                keyword,
                add,
            });
        })
    }

    pub(super) fn show_tag_menu(&self, sender: &ComponentSender<Self>) {
        if self.tags.is_empty() {
            return;
        }
        // Gmail's l asks for the menu with the button folded away or left
        // out of the toolbar: then it hangs from the middle of the header.
        let b: gtk::Widget = match self.button(ToolbarItem::Tags).filter(|b| b.is_mapped()) {
            Some(b) => b.clone().upcast(),
            None => self.bar.header.clone().upcast(),
        };
        show_context_menu(
            &b,
            f64::from(b.width() / 2),
            f64::from(b.height()),
            vec![self.tag_entries(sender)],
        );
    }

    /// Only folded actions, in saved order, with the main window's labels.
    pub(super) fn show_overflow_menu(&self, sender: &ComponentSender<Self>) {
        use MessageAction as A;
        let entry = |action: MessageAction, msg: fn() -> MessageWindowInput| {
            let s = sender.input_sender().clone();
            MenuEntry::new(action.label(), move || {
                let _ = s.send(msg());
            })
            .icon(format!("{}-symbolic", action.icon()))
        };
        let mut section = Vec::new();
        let items = self.toolbar.left.iter().chain(&self.toolbar.right);
        for item in items {
            if self.toolbar.fits(*item, self.toolbar_slots) {
                continue;
            }
            match item {
                ToolbarItem::Reply => section.push(entry(A::Reply, || MessageWindowInput::Reply)),
                ToolbarItem::ReplyAll => {
                    section.push(entry(A::ReplyAll, || MessageWindowInput::ReplyAll))
                }
                ToolbarItem::Forward => {
                    section.push(entry(A::Forward, || MessageWindowInput::Forward))
                }
                ToolbarItem::Star => section.push(entry(
                    if self.msg.starred {
                        A::RemoveStar
                    } else {
                        A::Star
                    },
                    || MessageWindowInput::ToggleStar,
                )),
                ToolbarItem::Archive => {
                    section.push(entry(A::Archive, || MessageWindowInput::Archive))
                }
                ToolbarItem::Delete => {
                    section.push(entry(A::Delete, || MessageWindowInput::Delete))
                }
                ToolbarItem::Spam => section.push(entry(
                    if self.in_junk {
                        A::NotSpam
                    } else {
                        A::MarkSpam
                    },
                    || MessageWindowInput::Spam,
                )),
                ToolbarItem::ReadUnread => section.push(entry(
                    if self.msg.unread {
                        A::MarkRead
                    } else {
                        A::MarkUnread
                    },
                    || MessageWindowInput::ToggleRead,
                )),
                ToolbarItem::Tags => {
                    if !self.tags.is_empty() {
                        section.push(
                            MenuEntry::submenu(A::Tags.label(), vec![self.tag_entries(sender)])
                                .icon("tag-outline-symbolic"),
                        );
                    }
                }
                ToolbarItem::MoveTo => {
                    if self.restorable {
                        section.push(entry(A::MoveToInbox, || MessageWindowInput::MoveToInbox));
                    }
                    section.push(entry(A::MoveTo, || MessageWindowInput::MoveTo));
                }
                ToolbarItem::Find => section.push(entry(A::Find, || MessageWindowInput::Find)),
                ToolbarItem::Print => {
                    section.push(entry(A::PrintPreview, || MessageWindowInput::PrintPreview))
                }
            }
        }
        let b = &self.bar.overflow;
        show_context_menu(
            b,
            f64::from(b.width() / 2),
            f64::from(b.height()),
            vec![section],
        );
    }
}
