//! The reader header's action buttons: their order, folding into the
//! overflow menu when the window is narrow, and the menu itself.

use adw::prelude::*;
use relm4::prelude::*;

use crate::config;
use crate::i18n::{i18n, i18n_f};
use crate::models::{FolderKind, Message};
use crate::ui::context_menu::{show_context_menu, MenuEntry};
use crate::ui::message_actions::MessageAction;
use crate::ui::preferences::PrefInput;
use crate::ui::toolbar_fit::ToolbarFit;

use super::model::AppModel;
use super::msg::AppMsg;
use super::AppModelWidgets;

/// The reader header's action buttons and width fitting, kept for
/// re-packing when the layout changes (see relayout_reader_toolbar).
pub(super) struct ReaderToolbarWidgets {
    /// Each action button inside its Focus Mode revealer.
    pub(super) buttons: Vec<(config::ToolbarItem, gtk::Widget)>,
    /// The Outbox's own buttons' revealers (Edit, Send, Send all).
    pub(super) outbox: Vec<gtk::Revealer>,
    spinner: gtk::Widget,
    pub(super) bin: adw::BreakpointBin,
    pub(super) fit: std::rc::Rc<ToolbarFit>,
}

/// A menu entry that sends `msg` to the app, built afresh on each click.
fn action_entry(
    sender: &ComponentSender<AppModel>,
    label: String,
    icon: &str,
    msg: impl Fn() -> AppMsg + 'static,
    enabled: bool,
) -> crate::ui::context_menu::MenuEntry {
    let s = sender.input_sender().clone();
    crate::ui::context_menu::MenuEntry::new(label, move || {
        let _ = s.send(msg());
    })
    .icon(format!("{icon}-symbolic"))
    .enabled(enabled)
}

impl AppModel {
    pub(super) fn remote_allowed(&self, m: &Message) -> bool {
        // A choice made for this one message wins over the standing policy,
        // in both directions: it is how remote content is shown (or put back)
        // for a single message when the banner is switched off.
        if let Some(&show) = self.remote_override.get(&(m.account_id, m.id)) {
            return show;
        }
        if self.auto_remote_content {
            return true;
        }
        let addr = m.from_addr.to_lowercase();
        self.allowed_senders.contains(&addr)
    }

    /// The tag entries of the reader's menus (#71): one per tag, its swatch
    /// filled where the reader's target message carries it. None without a
    /// target or a tag.
    pub(super) fn reader_tag_entries(
        &self,
        sender: &ComponentSender<Self>,
    ) -> Option<Vec<crate::ui::context_menu::MenuEntry>> {
        if self.tags.is_empty() {
            return None;
        }
        // Several selected (#313): a tag is ticked when every one carries
        // it, since only then does choosing it take it off.
        let many = self.multi_selection();
        let target = match many.first() {
            Some(first) => {
                let mut all = first.clone();
                all.keywords = first.tag_keywords(&self.tags);
                all.keywords
                    .retain(|k| many.iter().all(|m| self.carries_tag(m, k)));
                all
            }
            None => self.reply_target()?,
        };
        let s = sender.input_sender().clone();
        Some(crate::ui::message_list::tag_menu_entries(
            &self.tags,
            &target,
            move |keyword, _add| {
                let _ = s.send(AppMsg::ToggleTagCurrent(keyword));
            },
        ))
    }

    /// The actions that did not fit, with the same state as their buttons.
    fn show_reader_overflow_menu(&self, sender: &ComponentSender<Self>) {
        let has_current = self.current.is_some();
        let focus = self.focus.active(config::FocusPart::ReaderToolbar);
        let sections = if self.showing_outbox {
            self.outbox_overflow_sections(has_current, focus || self.compact, sender)
        } else {
            self.toolbar_overflow_sections(has_current, focus, sender)
        };

        let btn = &self.reader_overflow_btn;
        show_context_menu(btn, (btn.width() / 2) as f64, btn.height() as f64, sections);
    }

    /// Outbox actions hidden by phone navigation or Focus Mode, plus any
    /// folded Delete button and View Source (queued rows have no row menu).
    fn outbox_overflow_sections(
        &self,
        has_current: bool,
        focus: bool,
        sender: &ComponentSender<Self>,
    ) -> Vec<Vec<MenuEntry>> {
        let mut sections = Vec::new();
        let mut actions = Vec::new();
        if focus {
            actions.extend([
                action_entry(
                    sender,
                    i18n("Edit this message"),
                    "document-edit",
                    || AppMsg::EditCurrentOutbox,
                    has_current,
                ),
                action_entry(
                    sender,
                    i18n("Try to send this message now"),
                    "mail-send",
                    || AppMsg::SendCurrentOutbox,
                    has_current,
                ),
                action_entry(
                    sender,
                    i18n("Send all"),
                    "mail-send",
                    || AppMsg::RetryAllOutbox,
                    true,
                ),
            ]);
        }
        if self.focus.active(config::FocusPart::ReaderToolbar)
            || !self.toolbar_visible(config::ToolbarItem::Delete)
        {
            actions.push(action_entry(
                sender,
                i18n("Delete"),
                "user-trash",
                || AppMsg::Delete,
                has_current || self.many_selected(),
            ));
        }
        if !actions.is_empty() {
            sections.push(actions);
        }
        sections.push(vec![action_entry(
            sender,
            i18n("View Source"),
            "code",
            || AppMsg::ViewSource,
            has_current,
        )]);
        sections
    }

    /// The reader toolbar's overflow, in saved order. Per-message actions
    /// act on the reply target: the open message, or, in a conversation,
    /// the one highlighted card. With none (or several) highlighted they
    /// grey out.
    fn toolbar_overflow_sections(
        &self,
        has_current: bool,
        focus: bool,
        sender: &ComponentSender<Self>,
    ) -> Vec<Vec<MenuEntry>> {
        use config::ToolbarItem as T;
        use MessageAction as A;
        let target = self.reply_target();
        let acts = target.is_some();
        let many = acts || self.many_selected();
        let starred = target.as_ref().is_some_and(|m| m.starred);
        let target_unread = target.as_ref().is_some_and(|m| m.unread);
        let restorable = target.as_ref().is_some_and(|m| {
            self.folder_kind(m.account_id, m.folder_id)
                .is_some_and(|k| matches!(k, FolderKind::Trash | FolderKind::Junk))
        });
        let groups = [&self.reader_toolbar.left, &self.reader_toolbar.right];
        let item = |action: MessageAction, msg: fn() -> AppMsg, enabled: bool| {
            action_entry(sender, action.label(), action.icon(), msg, enabled)
        };
        let mut sections = Vec::new();
        for group in groups {
            let mut section = Vec::new();
            for item_kind in group {
                if !focus && self.toolbar_visible(*item_kind) {
                    continue;
                }
                match item_kind {
                    T::Reply => section.push(item(A::Reply, || AppMsg::Reply, acts)),
                    T::ReplyAll => section.push(item(A::ReplyAll, || AppMsg::ReplyAll, acts)),
                    T::Forward => section.push(item(A::Forward, || AppMsg::Forward, acts)),
                    T::Star => section.push(item(
                        if starred { A::RemoveStar } else { A::Star },
                        || AppMsg::ToggleStar,
                        many,
                    )),
                    T::Archive => section.push(item(A::Archive, || AppMsg::Archive, many)),
                    T::Delete => section.push(item(A::Delete, || AppMsg::Delete, many)),
                    T::Spam => section.push(item(
                        if self.target_in_junk() {
                            A::NotSpam
                        } else {
                            A::MarkSpam
                        },
                        || AppMsg::MarkSpam,
                        many,
                    )),
                    T::ReadUnread => section.push(item(
                        if target_unread {
                            A::MarkRead
                        } else {
                            A::MarkUnread
                        },
                        || AppMsg::ToggleReadCurrent,
                        many,
                    )),
                    // Tags (#71), where there are any and something to tag —
                    // behind a submenu, as in the message list's menu.
                    T::Tags => {
                        if let Some(entries) = self.reader_tag_entries(sender) {
                            section.push(
                                MenuEntry::submenu(A::Tags.label(), vec![entries])
                                    .icon("tag-outline-symbolic"),
                            );
                        }
                    }
                    T::MoveTo => {
                        if restorable {
                            section.push(item(A::MoveToInbox, || AppMsg::MoveToInbox, acts));
                        }
                        section.push(item(A::MoveTo, || AppMsg::MoveToMenu, many));
                    }
                    T::Find => section.push(item(A::Find, || AppMsg::OpenReaderFind, has_current)),
                    // View Source is deliberately absent: it lives in the
                    // message list's context menu only (the Outbox variant
                    // above keeps it — queued rows have no such menu).
                    T::Print => {
                        section.push(item(A::PrintPreview, || AppMsg::PrintPreview, has_current))
                    }
                }
            }
            if !section.is_empty() {
                sections.push(section);
            }
        }
        sections
    }

    /// Delete alone also serves the Outbox; the inline composer hides the
    /// whole row. Width fitting preserves the saved order on either side.
    pub(super) fn toolbar_visible(&self, item: config::ToolbarItem) -> bool {
        self.reader_compose.is_none()
            && (item == config::ToolbarItem::Delete || !self.showing_outbox)
            && self.reader_toolbar.fits(item, self.reader_toolbar_slots)
    }

    /// The ⋯ stands in for the folded buttons; the Outbox also keeps View
    /// Source here, since its queued messages have no row context menu.
    pub(super) fn reader_overflow_wanted(&self) -> bool {
        if self.reader_compose.is_some() {
            return false;
        }
        if self.showing_outbox || self.focus.active(config::FocusPart::ReaderToolbar) {
            return true;
        }
        self.reader_toolbar
            .left
            .iter()
            .chain(&self.reader_toolbar.right)
            .any(|item| {
                (*item != config::ToolbarItem::Tags || !self.tags.is_empty())
                    && !self.toolbar_visible(*item)
            })
    }

    /// Pack the header's action buttons in the saved order: the left group
    /// via pack_start, the right group via pack_end (which fills right to
    /// left, so in reverse), then the attachments spinner so it keeps the
    /// innermost slot of the right group. Buttons on neither side are left
    /// unpacked. Re-folds the row for the new count afterwards.
    pub(super) fn relayout_reader_toolbar(&self) {
        let (Some(header), Some(tb)) =
            (self.reader_header.get(), self.reader_toolbar_widgets.get())
        else {
            return;
        };
        for (_, w) in &tb.buttons {
            if w.parent().is_some() {
                header.remove(w);
            }
        }
        if tb.spinner.parent().is_some() {
            header.remove(&tb.spinner);
        }
        let find =
            |item: config::ToolbarItem| tb.buttons.iter().find(|(i, _)| *i == item).map(|(_, w)| w);
        // Each button's Focus Mode revealer slides towards the edge its
        // group sits against: GTK's SlideRight moves a folding child off to
        // the left, SlideLeft keeps it anchored and shrinks it from its
        // right, which is what an end-packed group needs.
        for item in &self.reader_toolbar.left {
            if let Some(w) = find(*item) {
                if let Some(r) = w.downcast_ref::<gtk::Revealer>() {
                    r.set_transition_type(gtk::RevealerTransitionType::SlideRight);
                }
                header.pack_start(w);
            }
        }
        for item in self.reader_toolbar.right.iter().rev() {
            if let Some(w) = find(*item) {
                if let Some(r) = w.downcast_ref::<gtk::Revealer>() {
                    r.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
                }
                header.pack_end(w);
            }
        }
        header.pack_end(&tb.spinner);
        tb.fit.set_layout(&self.reader_toolbar);
    }

    /// Tooltip for the toolbar's trash button: says when it will delete the
    /// whole multi-selection rather than just the open message.
    pub(super) fn delete_tooltip(&self) -> String {
        match self.list_selection.len() {
            n if n > 1 => i18n_f("Delete {n} messages", &[("n", &n.to_string())]),
            _ => i18n("Delete"),
        }
    }

    /// The messages the router sends here (see `AppModel::route`).
    pub(super) fn on_reader_toolbar(&mut self, msg: AppMsg, sender: ComponentSender<Self>) {
        match msg {
            AppMsg::SetReaderToolbarSlots(slots) => {
                self.reader_toolbar_slots = slots;
                self.reader_overflow_btn
                    .set_visible(self.reader_overflow_wanted());
            }

            AppMsg::ReaderOverflowMenu => self.show_reader_overflow_menu(&sender),
            AppMsg::ReaderToolbarMenu { x, y } => {
                use crate::ui::context_menu::{show_context_menu, MenuEntry};
                if let Some(header) = self.reader_header.get() {
                    let s = sender.input_sender().clone();
                    let entry = MenuEntry::new(i18n("Customize Toolbar…"), move || {
                        let _ = s.send(AppMsg::CustomizeToolbar);
                    })
                    .icon("preferences-desktop-appearance-symbolic");
                    show_context_menu(header, x, y, vec![vec![entry]]);
                }
            }
            AppMsg::CustomizeToolbar => {
                self.open_settings_window(&sender, false, false);
                if let Some(p) = &self.prefs {
                    p.emit(PrefInput::ShowPageById("appearance".to_string()));
                }
            }
            AppMsg::CustomizeColumns => {
                self.open_settings_window(&sender, false, false);
                if let Some(p) = &self.prefs {
                    p.emit(PrefInput::RevealColumns);
                }
            }
            _ => unreachable!("not routed to on_reader_toolbar"),
        }
    }

    /// Right-click on the header's empty space (not a button) offers
    /// the way to the toolbar editor.
    pub(super) fn init_reader_header_menu(
        &self,
        widgets: &AppModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        let click = gtk::GestureClick::new();
        click.set_button(3);
        // Capture phase: the header's own window handle (a child, so
        // earlier in the bubble phase) would otherwise take the click
        // for the compositor's window menu and never let it through.
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let header: gtk::Widget = widgets.reader_header.clone().upcast();
        let s = sender.input_sender().clone();
        click.connect_pressed(move |g, _, x, y| {
            // Anything clickable under the pointer keeps its own
            // right-click (or lack of one).
            let mut w = header.pick(x, y, gtk::PickFlags::DEFAULT);
            while let Some(cur) = w {
                if cur == header {
                    break;
                }
                if cur.is::<gtk::Button>() || cur.is::<gtk::WindowControls>() {
                    return;
                }
                w = cur.parent();
            }
            g.set_state(gtk::EventSequenceState::Claimed);
            let _ = s.send(AppMsg::ReaderToolbarMenu { x, y });
        });
        widgets.reader_header.add_controller(click);
    }

    /// Measure fitting rows with the real controls, fonts and touch sizing.
    pub(super) fn init_reader_toolbar_fold(
        &self,
        widgets: &AppModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        let buttons = toolbar_buttons(widgets);
        let s = sender.input_sender().clone();
        let fit = ToolbarFit::new(
            &widgets.reader_bin,
            &widgets.reader_header,
            &buttons,
            &self.reader_overflow_btn,
            &widgets.tb_spinner,
            move |slots| s.emit(AppMsg::SetReaderToolbarSlots(slots)),
        );
        let _ = self.reader_toolbar_widgets.set(ReaderToolbarWidgets {
            buttons,
            outbox: vec![
                widgets.tb_outbox_edit.clone(),
                widgets.tb_outbox_send.clone(),
                widgets.tb_outbox_send_all.clone(),
            ],
            spinner: widgets.tb_spinner.clone().upcast(),
            bin: widgets.reader_bin.clone(),
            fit,
        });
        // Packs the saved order and sets the real threshold.
        self.relayout_reader_toolbar();
        self.connect_toolbar_menus(sender);
    }

    /// The overflow, Tags and Move To buttons open their menus.
    fn connect_toolbar_menus(&self, sender: &ComponentSender<Self>) {
        let s = sender.input_sender().clone();
        self.reader_overflow_btn.connect_clicked(move |_| {
            let _ = s.send(AppMsg::ReaderOverflowMenu);
        });
        let s = sender.input_sender().clone();
        self.reader_tag_btn.connect_clicked(move |_| {
            let _ = s.send(AppMsg::ReaderTagMenu);
        });
        let s = sender.input_sender().clone();
        self.reader_move_btn.connect_clicked(move |_| {
            let _ = s.send(AppMsg::MoveToMenu);
        });
    }
}

/// The reader toolbar's buttons, each with the item it stands for.
fn toolbar_buttons(widgets: &AppModelWidgets) -> Vec<(config::ToolbarItem, gtk::Widget)> {
    use config::ToolbarItem as T;
    vec![
        (T::Reply, widgets.tb_reply.clone().upcast()),
        (T::ReplyAll, widgets.tb_reply_all.clone().upcast()),
        (T::Forward, widgets.tb_forward.clone().upcast()),
        (T::Star, widgets.tb_star.clone().upcast()),
        (T::Archive, widgets.tb_archive.clone().upcast()),
        (T::Delete, widgets.tb_delete.clone().upcast()),
        (T::Spam, widgets.tb_spam.clone().upcast()),
        (T::ReadUnread, widgets.tb_read.clone().upcast()),
        (T::Tags, widgets.tb_tags.clone().upcast()),
        (T::MoveTo, widgets.tb_move.clone().upcast()),
        (T::Find, widgets.tb_find.clone().upcast()),
        (T::Print, widgets.tb_print.clone().upcast()),
    ]
}
