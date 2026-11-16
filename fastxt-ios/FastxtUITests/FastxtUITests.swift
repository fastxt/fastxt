//
//  FastxtUITests.swift
//  FastxtUITests
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import XCTest

/// The phone's way through: write a prompt on the dictation screen, find it
/// under Prompts, open it, and reach the sheet that sends it.
///
/// Speaking is not tested here: the simulator has no voice to hear. The text
/// is typed into the same screen instead.
///
/// With `TEST_RUNNER_FASTXT_PAIRING_CODE` set to the code of a running
/// `fastxt serve --agents`, the pairing test pairs with it and lists the
/// sessions open there. It never sends to one.
final class FastxtUITests: XCTestCase {
    private var app: XCUIApplication!

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launch()
    }

    func testAPromptIsWrittenFoundAndReadyToSend() {
        let text = "Add a retry test for the login flow \(Int.random(in: 1000...9999))"
        dictate(text)
        snap("1-home-after-save")

        app.buttons["Prompts"].tap()
        let row = element(containing: text)
        XCTAssertTrue(row.waitForExistence(timeout: 5), "the new prompt is under Prompts")
        app.buttons["Ideas"].tap()
        XCTAssertFalse(element(containing: text).waitForExistence(timeout: 2), "and not under Ideas")
        app.buttons["Prompts"].tap()
        snap("2-prompts")

        row.tap()
        let send = app.buttons["send"]
        XCTAssertTrue(send.waitForExistence(timeout: 5))
        snap("3-note")

        send.tap()
        let close = app.buttons["close-send"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        if PairedCode.value == nil {
            XCTAssertTrue(app.buttons["pair-computer"].waitForExistence(timeout: 5),
                          "an unpaired phone is asked to pair")
        }
        snap("4-send-sheet")
        close.tap()

        app.navigationBars.buttons.element(boundBy: 0).tap()
        delete(text)
    }

    func testPairingListsTheComputersSessions() throws {
        guard let code = PairedCode.value else {
            throw XCTSkip("set TEST_RUNNER_FASTXT_PAIRING_CODE to the code `fastxt serve --agents` shows")
        }
        app.buttons["computer"].tap()
        let field = app.descendants(matching: .any).matching(identifier: "pairing-code").firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText(code)
        app.buttons["pair"].tap()
        let status = app.staticTexts["pair-status"]
        XCTAssertTrue(waitFor(status, labelPrefix: "Paired.", timeout: 20), "paired: \(status.label)")
        snap("5-paired")
        app.buttons["done"].tap()

        let text = "List the open sessions \(Int.random(in: 1000...9999))"
        dictate(text)
        element(containing: text).tap()
        app.buttons["send"].tap()
        XCTAssertTrue(app.buttons["close-send"].waitForExistence(timeout: 5))
        let listed = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'session-'")).firstMatch
        let none = app.staticTexts["No agent session is open in Orca on your computer."]
        let deadline = Date().addingTimeInterval(20)
        while !listed.exists && !none.exists && Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        }
        XCTAssertTrue(listed.exists || none.exists, "the computer's sessions are listed")
        snap("6-sessions")
        app.buttons["close-send"].tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        delete(text)

        // Leave the simulator unpaired.
        app.buttons["computer"].tap()
        app.buttons["forget"].tap()
        app.buttons["done"].tap()
    }

    // MARK: Steps

    /// Save a prompt from the dictation screen, typed in place of spoken.
    private func dictate(_ text: String) {
        let dictate = app.buttons["dictate"]
        XCTAssertTrue(dictate.waitForExistence(timeout: 10))
        dictate.tap()
        XCTAssertTrue(app.buttons["record"].waitForExistence(timeout: 5))
        let save = app.buttons["save-prompt"]
        XCTAssertFalse(save.isEnabled, "nothing to save yet")
        let editor = app.textViews["dictated-text"]
        editor.tap()
        editor.typeText(text)
        snap("dictate-typing")
        XCTAssertTrue(save.isEnabled)
        XCTAssertTrue(save.isHittable, "the keyboard leaves the save buttons in reach")
        let done = app.buttons["keyboard-done"]
        if done.exists {
            done.tap()
            XCTAssertTrue(app.buttons["record"].waitForExistence(timeout: 5), "the microphone is back")
            snap("dictate-typed")
        }
        save.tap()
        XCTAssertTrue(element(containing: text).waitForExistence(timeout: 5), "the new note is listed")
    }

    /// Delete a note from the list, confirming it.
    private func delete(_ text: String) {
        let row = element(containing: text)
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.swipeLeft()
        app.buttons["Delete"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["Delete this note?"].waitForExistence(timeout: 5), "deleting asks first")
        snap("delete-confirmation")
        let deletes = app.buttons.matching(identifier: "Delete")
        deletes.element(boundBy: deletes.count - 1).tap()
        XCTAssertFalse(element(containing: text).waitForExistence(timeout: 2), "the note is gone")
    }

    private func element(containing text: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", text)).firstMatch
    }

    private func waitFor(_ element: XCUIElement, labelPrefix: String, timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if element.exists && element.label.hasPrefix(labelPrefix) {
                return true
            }
            RunLoop.current.run(until: Date().addingTimeInterval(0.5))
        }
        return false
    }

    private func snap(_ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}

private enum PairedCode {
    static var value: String? {
        let code = ProcessInfo.processInfo.environment["FASTXT_PAIRING_CODE"] ?? ""
        return code.isEmpty ? nil : code
    }
}
