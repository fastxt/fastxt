extern crate gio;
extern crate glib;
extern crate gtk;

use gio::prelude::*;
use glib::clone;
use gtk::prelude::*;
use gtk::{
    ApplicationWindow, ButtonsType, CellRendererPixbuf, CellRendererText, DialogFlags,
    MessageDialog, MessageType, Orientation, TextView, TreeStore, TreeView, TreeViewColumn,
    WindowPosition,
};

use std::env::args;

fn append_text_column(tree: &TreeView) {
    let column = TreeViewColumn::new();
    let cell = CellRendererText::new();

    column.pack_start(&cell, true);
    column.add_attribute(&cell, "text", 0);
    tree.append_column(&column);
}

fn build_ui(application: &gtk::Application) {
    let window = ApplicationWindow::new(application);

    window.set_title("Fastxt");
    window.set_position(WindowPosition::Center);

    // left pane
    let left_tree = TreeView::new();
    let left_store = TreeStore::new(&[String::static_type(), String::static_type()]);

    left_tree.set_model(Some(&left_store));
    left_tree.set_headers_visible(false);
    append_text_column(&left_tree);

    for i in 0..10 {
        // insert_with_values takes two slices: column indices and ToValue
        // trait objects. ToValue is implemented for strings, numeric types,
        // bool and Object descendants
        let iter = left_store.insert_with_values(None, None, &[0], &[&format!("Hello {}", i)]);
    }

    // right pane
    let text_view: gtk::TextView = TextView::new();
    // selection and path manipulation

    let left_selection = left_tree.get_selection();
    left_selection.connect_changed(clone!(@weak text_view => move |tree_selection| {
        text_view
            .get_buffer()
            .expect("Couldn't get window")
            .set_text(&"txt view".to_string());
    }));

    // display the panes

    let split_pane = gtk::Box::new(Orientation::Horizontal, 10);

    split_pane.set_size_request(-1, -1);
    split_pane.add(&left_tree);
    split_pane.add(&text_view);

    window.add(&split_pane);
    window.show_all();
}

fn main() {
    let application = gtk::Application::new(Some("app.fastxt.gtk"), Default::default())
        .expect("Initialization failed...");

    application.connect_activate(|app| {
        build_ui(app);
    });

    application.run(&args().collect::<Vec<_>>());
}
