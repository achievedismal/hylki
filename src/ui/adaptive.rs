//! Shared desktop panes and phone navigation. MultiLayoutView moves the live
//! widgets, including WebKit and its editing history, between the two layouts.

use adw::prelude::*;
use std::{cell::Cell, rc::Rc};

pub(crate) const MIN_WIDTH: i32 = 320;
/// Leave room for a window's borders and any surrounding content margins.
pub(crate) const PANE_MIN_WIDTH: i32 = 280;
pub(crate) const MIN_HEIGHT: i32 = 240;
pub(crate) const COMPACT_WIDTH: f64 = 900.0;

pub(crate) fn max_width(width: f64) -> adw::Breakpoint {
    adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        width,
        adw::LengthUnit::Sp,
    ))
}

/// A control row keeps its single line while it fits, wrapping at the actual
/// pane width. Unlike a BreakpointBin, WrapBox measures its full wrapped height.
pub(crate) fn wrap_controls(row: &gtk::Box) {
    let wrapped = adw::WrapBox::new();
    wrapped.set_child_spacing(row.spacing());
    wrapped.set_line_spacing(6);
    while let Some(child) = row.first_child() {
        row.remove(&child);
        wrapped.append(&child);
    }
    row.set_orientation(gtk::Orientation::Vertical);
    row.append(&wrapped);
}

/// Group header actions share a line with the title on desktop. On a phone
/// they go below the rows, giving long translated headings their full width.
pub(crate) fn stack_group_actions(widget: &impl IsA<gtk::Widget>, breakpoint: &adw::Breakpoint) {
    let widget = widget.as_ref();
    if let Some(group) = widget.downcast_ref::<adw::PreferencesGroup>() {
        if let Some(action) = group.header_suffix() {
            let margin = action.margin_start();
            let (g, a) = (group.downgrade(), action.downgrade());
            breakpoint.connect_apply(move |_| {
                if let (Some(group), Some(action)) = (g.upgrade(), a.upgrade()) {
                    group.set_header_suffix(None::<&gtk::Widget>);
                    action.set_margin_start(0);
                    group.add(&action);
                }
            });
            let (g, a) = (group.downgrade(), action.downgrade());
            breakpoint.connect_unapply(move |_| {
                if let (Some(group), Some(action)) = (g.upgrade(), a.upgrade()) {
                    group.remove(&action);
                    action.set_margin_start(margin);
                    group.set_header_suffix(Some(&action));
                }
            });
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        stack_group_actions(&c, breakpoint);
    }
}

pub(crate) struct AdaptivePanes {
    pub(crate) view: adw::MultiLayoutView,
    pub(crate) navigation: adw::NavigationSplitView,
    pub(crate) detail: adw::NavigationPage,
    paned: gtk::Paned,
    compact: Rc<Cell<bool>>,
    position: Rc<Cell<i32>>,
}

impl AdaptivePanes {
    /// The caller removes `paned` from its old parent before wrapping it.
    pub(crate) fn new(
        paned: &gtk::Paned,
        list_title: &str,
        detail_title: &str,
        breakpoint_sp: f64,
    ) -> Self {
        let list = paned.start_child().expect("list pane");
        let detail = paned.end_child().expect("detail pane");
        let position = paned.position();
        paned.set_start_child(None::<&gtk::Widget>);
        paned.set_end_child(None::<&gtk::Widget>);
        paned.set_start_child(Some(&adw::LayoutSlot::new("list")));
        paned.set_end_child(Some(&adw::LayoutSlot::new("detail")));
        let navigation = adw::NavigationSplitView::new();
        navigation.set_collapsed(true);
        navigation.set_show_content(false);
        let list_page = adw::NavigationPage::new(&adw::LayoutSlot::new("list"), list_title);
        let detail_page = adw::NavigationPage::new(&adw::LayoutSlot::new("detail"), detail_title);
        navigation.set_sidebar(Some(&list_page));
        navigation.set_content(Some(&detail_page));
        let view = adw::MultiLayoutView::new();
        let wide = adw::Layout::new(paned);
        wide.set_name(Some("wide"));
        view.add_layout(wide);
        let phone = adw::Layout::new(&navigation);
        phone.set_name(Some("compact"));
        view.add_layout(phone);
        view.set_child("list", &list);
        view.set_child("detail", &detail);
        let compact = Rc::new(Cell::new(false));
        let position = Rc::new(Cell::new(position));
        let (saved, folded) = (position.clone(), compact.clone());
        paned.connect_position_notify(move |p| {
            // Adwaita snapshots the outgoing layout at the new size before
            // applying a breakpoint. That squeeze is not the chosen divider.
            // The breakpoint bin also gives the emulated preview's width.
            let width = p
                .ancestor(adw::BreakpointBin::static_type())
                .map(|bin| bin.width())
                .unwrap_or_else(|| p.width());
            let threshold = adw::LengthUnit::Sp.to_px(breakpoint_sp, Some(&p.settings()));
            if !folded.get() && p.is_mapped() && f64::from(width) > threshold {
                saved.set(p.position());
            }
        });
        Self {
            view,
            navigation,
            detail: detail_page,
            paned: paned.clone(),
            compact,
            position,
        }
    }

    pub(crate) fn set_compact(&self, compact: bool) {
        if self.compact.replace(compact) == compact {
            return;
        }
        if compact {
            self.view.set_layout_name("compact");
        } else {
            self.view.set_layout_name("wide");
            // During unapply the paned still has its phone-sized allocation.
            // Restoring now would clamp the saved divider to it.
            let (paned, view, position) = (
                self.paned.downgrade(),
                self.view.downgrade(),
                self.position.get(),
            );
            gtk::glib::idle_add_local_once(move || {
                if let (Some(paned), Some(view)) = (paned.upgrade(), view.upgrade()) {
                    if view.layout_name().as_deref() == Some("wide") {
                        paned.set_position(position);
                    }
                }
            });
        }
    }

    pub(crate) fn show_detail(&self, show: bool) {
        self.navigation.set_show_content(show);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn settle() {
        let end = Instant::now() + Duration::from_millis(350);
        let context = gtk::glib::MainContext::default();
        while Instant::now() < end {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    // Run under a desktop session or Xvfb, on one test thread. This exercises
    // real allocation and reparenting rather than only the breakpoint policy.
    #[test]
    #[ignore = "requires a GTK display"]
    fn phone_navigation_preserves_live_content_and_desktop_divider() {
        adw::init().unwrap();
        let list = gtk::Entry::new();
        list.set_text("Inbox search");
        let detail = gtk::TextView::new();
        detail.buffer().set_text("Unsent reply");
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&list));
        paned.set_end_child(Some(&detail));
        paned.set_resize_start_child(false);
        paned.set_position(360);
        let panes = Rc::new(AdaptivePanes::new(&paned, "Messages", "Message", 760.0));
        let window = adw::Window::new();
        window.set_size_request(MIN_WIDTH, MIN_HEIGHT);
        window.set_default_size(1100, 640);
        window.set_content(Some(&panes.view));
        let bp = max_width(760.0);
        let p = panes.clone();
        bp.connect_apply(move |_| p.set_compact(true));
        let p = panes.clone();
        bp.connect_unapply(move |_| p.set_compact(false));
        window.add_breakpoint(bp);
        window.present();
        settle();
        paned.set_position(410);
        settle();
        let divider = paned.position();
        assert!(list.is_mapped() && detail.is_mapped());
        window.set_default_size(360, 640);
        settle();
        assert!(list.is_mapped());
        assert!(!detail.is_mapped());
        panes.show_detail(true);
        settle();
        assert!(detail.is_mapped());
        detail.buffer().set_text("Reply edited on the phone");
        for width in [1100, 320, 1100, 430] {
            window.set_default_size(width, 640);
            settle();
            assert_eq!(list.text(), "Inbox search");
            let buffer = detail.buffer();
            assert_eq!(
                buffer.text(&buffer.start_iter(), &buffer.end_iter(), false),
                "Reply edited on the phone"
            );
            assert!(detail.is_mapped());
            if width == 1100 {
                assert_eq!(paned.position(), divider);
            }
        }
        panes.show_detail(false);
        settle();
        // Rotation through a wide layout must not reopen the last message.
        for width in [1100, 360] {
            window.set_default_size(width, 640);
            settle();
        }
        assert!(list.is_mapped() && !detail.is_mapped());
        panes.show_detail(true);
        settle();
        assert!(detail.is_mapped(), "the same message can be reopened");
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        for _ in 0..5 {
            controls.append(&gtk::Button::with_label("Attachment action"));
        }
        wrap_controls(&controls);
        window.set_content(Some(&controls));
        settle();
        let wide_height = controls.measure(gtk::Orientation::Vertical, 1100).0;
        let phone_height = controls.measure(gtk::Orientation::Vertical, 320).0;
        assert!(
            phone_height > wide_height,
            "wrapped controls reserve their full height"
        );
        window.destroy();
    }
}
