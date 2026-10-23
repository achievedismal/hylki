//! A wrapping footer and thumbnail-only view at phone widths.

use super::{AttachmentsGallery, AttachmentsGalleryWidgets, GalleryInput};
use adw::prelude::*;
use relm4::prelude::*;

impl AttachmentsGallery {
    pub(super) fn table_mode(&self) -> bool {
        self.view_table && !self.compact
    }

    pub(super) fn init_adaptive(
        &self,
        root: &adw::BreakpointBin,
        widgets: &AttachmentsGalleryWidgets,
        sender: &ComponentSender<Self>,
    ) {
        let parent = widgets
            .footer
            .parent()
            .and_downcast::<gtk::Box>()
            .expect("gallery footer");
        parent.remove(&widgets.footer);
        let layouts = adw::MultiLayoutView::new();
        let wrapped = adw::WrapBox::new();
        wrapped.set_child_spacing(8);
        wrapped.set_line_spacing(6);
        wrapped.add_css_class("gallery-footer");
        for (i, (child, start)) in [
            (widgets.view_buttons.clone().upcast::<gtk::Widget>(), true),
            (widgets.account_dropdown.clone().upcast(), true),
            (widgets.folders_button.clone().upcast(), true),
            (widgets.type_dropdown.clone().upcast(), true),
            (widgets.sort_dropdown.clone().upcast(), true),
            (widgets.count_label.clone().upcast(), false),
            (widgets.scan_status.clone().upcast(), false),
            (widgets.thumb_scale.clone().upcast(), false),
        ]
        .into_iter()
        .enumerate()
        {
            widgets.footer.remove(&child);
            let id = i.to_string();
            let slot = adw::LayoutSlot::new(&id);
            if start {
                widgets.footer.pack_start(&slot);
            } else {
                widgets.footer.pack_end(&slot);
            }
            wrapped.append(&adw::LayoutSlot::new(&id));
            layouts.set_child(&id, &child);
        }
        for (name, widget) in [
            ("wide", widgets.footer.upcast_ref::<gtk::Widget>()),
            ("compact", wrapped.upcast_ref()),
        ] {
            let layout = adw::Layout::new(widget);
            layout.set_name(Some(name));
            layouts.add_layout(layout);
        }
        let compact = crate::ui::adaptive::max_width(900.0);
        compact.add_setter(&layouts, "layout-name", Some(&"compact".to_value()));
        let s = sender.clone();
        compact.connect_apply(move |_| s.input(GalleryInput::SetCompact(true)));
        let s = sender.clone();
        compact.connect_unapply(move |_| s.input(GalleryInput::SetCompact(false)));
        root.add_breakpoint(compact);
        parent.append(&layouts);
    }
}
