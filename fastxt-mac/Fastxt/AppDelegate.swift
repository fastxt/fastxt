//
//  AppDelegate.swift
//  Fastxt
//
//  Created by Yi Wang on 12/6/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import Cocoa
import SwiftUI

@NSApplicationMain
class AppDelegate: NSObject, NSApplicationDelegate {

    var window: NSWindow!


    func applicationDidFinishLaunching(_ aNotification: Notification) {
        // Create the SwiftUI view that provides the window contents.
        let contentView = ContentView()

        // Create the window and set the content view. 
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 300),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered, defer: false)
        window.center()
        window.setFrameAutosaveName("Main Window")
        window.contentView = NSHostingView(rootView: contentView)
        window.makeKeyAndOrderFront(nil)
        let ft = RustFastxt()
        let r = ft.run(json_input:"""
            {"action":"insert","limit":10,"offset":0,"txt":"new text", "tags":"new tag"}
            """
        )
        print(r)
        let txt = ft.run(json_input:"""
            {"action":"select","limit":10,"offset":0}
            """
        )
        print(txt)
    }

    func applicationWillTerminate(_ aNotification: Notification) {
        // Insert code here to tear down your application
    }


}

