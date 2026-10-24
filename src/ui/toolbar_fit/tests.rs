use super::*;
use std::time::{Duration, Instant};

fn settle() {
    let until = Instant::now() + Duration::from_millis(350);
    let context = gtk::glib::MainContext::default();
    while Instant::now() < until {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

// Exercise real GTK measurements, including Back, decorations, the nested
// menu buttons and the revealers used by Focus Mode. Run on one test thread.
#[test]
#[ignore = "requires a GTK display"]
fn reader_actions_fit_and_restore_across_widths_and_customization() {
    adw::init().unwrap();
    let css = gtk::CssProvider::new();
    css.load_from_string(include_str!("../../styles.css"));
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    for wrapped in [true, false] {
        check_toolbar(wrapped);
    }
}

fn check_toolbar(wrapped: bool) {
    use ToolbarItem as T;
    let window = adw::Window::new();
    window.set_default_size(900, 240);
    let header = adw::HeaderBar::new();
    header.add_css_class("reader-toolbar");
    header.set_title_widget(Some(&gtk::Label::new(Some(""))));
    header.set_decoration_layout(Some(":close"));
    header.set_show_back_button(wrapped);
    let overflow = gtk::Button::from_icon_name("view-more-horizontal-symbolic");
    overflow.add_css_class("flat");
    overflow.set_visible(false);
    header.pack_end(&overflow);
    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    let layout = Rc::new(RefCell::new(ReaderToolbar::default()));
    let buttons: Vec<_> = T::ALL
        .iter()
        .map(|item| {
            let button = gtk::Button::from_icon_name(item.icon());
            button.add_css_class("flat");
            let widget: gtk::Widget = if wrapped {
                let r = gtk::Revealer::new();
                r.set_transition_duration(0);
                r.set_reveal_child(true);
                if matches!(item, T::Tags | T::MoveTo) {
                    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    holder.append(&button);
                    r.set_child(Some(&holder));
                } else {
                    r.set_child(Some(&button));
                }
                r.upcast()
            } else {
                button.clone().upcast()
            };
            (*item, widget, button)
        })
        .collect();
    let pack = |layout: &ReaderToolbar| {
        for (_, w, _) in &buttons {
            if w.parent().is_some() {
                header.remove(w);
            }
        }
        if spinner.parent().is_some() {
            header.remove(&spinner);
        }
        for item in &layout.left {
            header.pack_start(&buttons.iter().find(|(i, _, _)| i == item).unwrap().1);
        }
        for item in layout.right.iter().rev() {
            header.pack_end(&buttons.iter().find(|(i, _, _)| i == item).unwrap().1);
        }
        header.pack_end(&spinner);
    };
    pack(&layout.borrow());
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&gtk::Label::new(Some("Message"))));
    let bin = adw::BreakpointBin::new();
    bin.set_size_request(280, 200);
    bin.set_child(Some(&view));
    let navigation = adw::NavigationSplitView::new();
    navigation.set_collapsed(true);
    navigation.set_sidebar(Some(&adw::NavigationPage::new(
        &gtk::Label::new(Some("Messages")),
        "Messages",
    )));
    navigation.set_content(Some(&adw::NavigationPage::new(&bin, "Message")));
    navigation.set_show_content(true);
    // Allocate a pane inside a fixed window: this also works on compositors
    // that ignore a mapped window's requested default size.
    let host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    navigation.set_hexpand(false);
    navigation.set_width_request(800);
    host.append(&navigation);
    window.set_content(Some(&host));
    let slots = Rc::new(Cell::new(usize::MAX));
    let show = {
        let buttons = buttons.clone();
        let overflow = overflow.clone();
        let layout = layout.clone();
        let slots = slots.clone();
        move |n| {
            slots.set(n);
            for (item, widget, _) in &buttons {
                widget.set_visible(layout.borrow().fits(*item, n));
            }
            overflow.set_visible(n < layout.borrow().priority().len());
        }
    };
    let fit = ToolbarFit::new(
        &bin,
        &header,
        &buttons
            .iter()
            .map(|(i, w, _)| (*i, w.clone()))
            .collect::<Vec<_>>(),
        &overflow,
        &spinner,
        show.clone(),
    );
    fit.set_layout(&layout.borrow());
    window.present();
    let visible = |item| {
        buttons
            .iter()
            .find(|(i, _, _)| *i == item)
            .unwrap()
            .1
            .is_visible()
    };
    for width in [800, 440, 360, 320, 440, 800] {
        if width < 500 {
            window.add_css_class("mobile");
        } else {
            window.remove_css_class("mobile");
        }
        fit.refresh();
        navigation.set_width_request(width);
        settle();
        // Headless or occluded surfaces can stop their frame clocks. Drive
        // the real allocation explicitly so their resize policy is tested.
        bin.allocate(width, 200, -1, None);
        assert!(
            bin.width() <= width,
            "toolbar raised the pane's minimum: requested {width}, window {}, bin {}, header {:?}, slots {}",
            window.width(), bin.width(), header.measure(gtk::Orientation::Horizontal, -1), slots.get()
        );
        assert!(
            header.measure(gtk::Orientation::Horizontal, -1).0 <= bin.width(),
            "header clips at {width}px (wrapped={wrapped}, slots={})",
            slots.get()
        );
        for item in [T::Reply, T::ReplyAll] {
            assert!(
                visible(item),
                "{item:?} folded at {width}px (wrapped={wrapped})"
            );
        }
        if visible(T::Forward) || visible(T::Star) {
            assert!(
                visible(T::Archive) && visible(T::Delete),
                "filing actions must outlast the lower-priority buttons"
            );
        }
        if width >= 440 {
            for item in &layout.borrow().left {
                assert!(
                    visible(*item),
                    "primary action {item:?} folded at {width}px"
                );
            }
        }
        if width == 800 {
            assert!(buttons.iter().all(|(_, w, _)| w.is_visible()));
            assert!(!overflow.is_visible());
        } else {
            assert!(overflow.is_visible());
        }
    }
    // Moving Reply All to the other group must not lower its priority,
    // and an explicitly hidden Reply must not be brought back by fitting.
    {
        let mut layout = layout.borrow_mut();
        layout.place(T::Print, None, 0);
        layout.place(T::ReplyAll, Some(crate::config::ToolbarSide::Right), 0);
        layout.place(T::Reply, None, 0);
        pack(&layout);
        fit.set_layout(&layout);
    }
    show(slots.get());
    window.add_css_class("mobile");
    navigation.set_width_request(320);
    settle();
    bin.allocate(320, 200, -1, None);
    assert!(visible(T::ReplyAll));
    assert!(!visible(T::Reply));
    assert!(header.measure(gtk::Orientation::Horizontal, -1).0 <= bin.width());

    // Text scaling and a three-button decoration layout change real costs.
    let settings = gtk::Settings::default().unwrap();
    let dpi = settings.gtk_xft_dpi();
    settings.set_gtk_xft_dpi(144 * 1024);
    header.set_decoration_layout(Some(":minimize,maximize,close"));
    fit.refresh();
    settle();
    bin.allocate(320, 200, -1, None);
    assert!(visible(T::ReplyAll));
    assert!(
        header.measure(gtk::Orientation::Horizontal, -1).0 <= bin.width(),
        "scaled header {:?} exceeds {} with {} slots; thresholds {:?}",
        header.measure(gtk::Orientation::Horizontal, -1),
        bin.width(),
        slots.get(),
        fit.breakpoints
            .iter()
            .map(|bp| bp.condition().map(|c| c.to_string()))
            .collect::<Vec<_>>()
    );
    if wrapped {
        for (_, widget, _) in &buttons {
            widget
                .downcast_ref::<gtk::Revealer>()
                .unwrap()
                .set_reveal_child(false);
        }
        fit.refresh();
        settle();
        assert!(
            buttons.iter().all(|(_, widget, _)| {
                !widget
                    .downcast_ref::<gtk::Revealer>()
                    .unwrap()
                    .reveals_child()
            }),
            "measuring the toolbar must preserve Focus Mode"
        );
    }
    settings.set_gtk_xft_dpi(dpi);
    window.destroy();
    settle();
}
