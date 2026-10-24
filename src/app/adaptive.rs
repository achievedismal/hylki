//! Switching the live mail panes between desktop and phone navigation.

use adw::prelude::*;
use relm4::prelude::*;

use crate::i18n::i18n;
use crate::ui::adaptive::{AdaptivePanes, COMPACT_WIDTH, PANE_MIN_WIDTH};
use crate::ui::contacts_page::ContactsPageInput;
use crate::ui::message_list::MessageListInput;

use super::{consts::READER_MIN_WIDTH, model::AppModel, msg::AppMsg, AppModelWidgets};

impl AppModel {
    pub(super) fn init_adaptive_panes(
        &mut self,
        widgets: &AppModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        // Keep the desktop divider and its saved width, while libadwaita owns
        // the phone's back stack. The list and reader themselves stay alive.
        widgets.content_stack.remove(&widgets.mail_paned);
        let panes = AdaptivePanes::new(
            &widgets.mail_paned,
            &i18n("Messages"),
            &i18n("Message"),
            COMPACT_WIDTH,
        );
        let s = sender.clone();
        panes.navigation.connect_show_content_notify(move |nav| {
            if !nav.shows_content() {
                s.input(AppMsg::MobileBack);
            }
        });
        widgets.content_stack.add_named(&panes.view, Some("mail"));
        self.mail_panes = Some(panes);
        self.list_header = Some(widgets.list_header.clone());
    }

    pub(super) fn set_compact(&mut self, on: bool) {
        if self.compact == on {
            return;
        }
        self.compact = on;
        if !on {
            self.auto_rail = self.window.width() <= 1094;
        }
        if let Some(anim) = self.sidebar_anim.take() {
            anim.skip();
        }
        self.set_sidebar_peek(false, false);
        if let Some(split) = &self.sidebar_split {
            split.set_collapsed(on);
            split.set_show_sidebar(!on);
        }
        if let Some(peek) = &self.peek_split {
            peek.set_min_sidebar_width(if on { 180.0 } else { 280.0 });
            peek.set_sidebar_width_fraction(if on { 0.9 } else { 0.25 });
        }
        if let Some(panes) = &self.mail_panes {
            panes.set_compact(on);
        }
        if let Some(header) = &self.list_header {
            header.set_show_start_title_buttons(on);
            header.set_show_end_title_buttons(on);
            header.set_decoration_layout(if on { Some(":close") } else { None });
            if on {
                header.set_title_widget(None::<&gtk::Widget>);
            } else {
                header.set_title_widget(Some(&gtk::Label::new(Some(""))));
            }
        }
        if let Some(header) = self.reader_header.get() {
            header.set_decoration_layout(if on { Some(":close") } else { None });
        }
        if let Some(tb) = self.reader_toolbar_widgets.get() {
            tb.bin
                .set_width_request(if on { PANE_MIN_WIDTH } else { READER_MIN_WIDTH });
        }
        if on {
            self.window.add_css_class("mobile");
        } else {
            self.window.remove_css_class("mobile");
        }
        if let Some(tb) = self.reader_toolbar_widgets.get() {
            tb.fit.refresh();
        }
        self.message_list.emit(MessageListInput::SetCompact(on));
        self.contacts_page.emit(ContactsPageInput::SetCompact(on));
        self.push_single_line();
        self.sync_focus_chrome();
        self.reader_overflow_btn
            .set_visible(self.reader_overflow_wanted());
        // Moving an open split reply keeps its buffer and undo history.
        if on {
            if let Some(widget) = self
                .reader_compose
                .as_ref()
                .filter(|r| r.window.is_none())
                .map(|r| r.controller.widget().clone())
            {
                self.clear_compose_slots();
                let slot = self.compose_slot();
                slot.set_child(Some(&widget));
                slot.set_can_target(true);
                slot.set_reveal_child(true);
            }
        } else {
            self.sync_rail();
        }
    }
}
