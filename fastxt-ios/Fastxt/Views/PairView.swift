/*
 Fastxt
 Copyright (C) 2026  Yi Wang

 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU Affero General Public License as published by
 the Free Software Foundation, either version 3 of the License, or
 (at your option) any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU Affero General Public License for more details.

 You should have received a copy of the GNU Affero General Public License
 along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

import SwiftUI

/// The computer this phone syncs with and sends prompts to: pair with it,
/// sync now, or forget it.
struct PairView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var current = PairedComputer.load()
    @State private var input = ""
    @State private var scanning = false
    @State private var busy = false
    @State private var status = ""

    var body: some View {
        NavigationStack {
            Form {
                if let current = current {
                    Section(header: Text("Paired computer")) {
                        Text(PairedComputer.address(current))
                            .font(.system(.body, design: .monospaced))
                        Button("Sync notes now") { sync(current) }
                            .disabled(busy)
                        Button("Forget this computer", role: .destructive) {
                            PairedComputer.forget()
                            self.current = nil
                            status = ""
                        }
                        .accessibilityIdentifier("forget")
                    }
                }
                Section(header: Text(current == nil ? "Pair with your computer" : "Pair again"),
                        footer: Text("On your computer run `fastxt serve --agents`, then scan or paste the code it shows. A code lasts until that command stops.")) {
                    TextField("FASTXT1:…", text: $input, axis: .vertical)
                        .font(.system(.footnote, design: .monospaced))
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .accessibilityIdentifier("pairing-code")
                    HStack {
                        #if !targetEnvironment(simulator)
                        Button("Scan the code") { scanning = true }
                            .buttonStyle(.borderless)
                        #endif
                        Spacer()
                        Button("Pair") { pair() }
                            .buttonStyle(.borderless)
                            .disabled(busy || !PairedComputer.isPairingCode(input))
                            .accessibilityIdentifier("pair")
                    }
                }
                if busy || !status.isEmpty {
                    Section {
                        if busy {
                            ProgressView()
                        }
                        Text(status)
                            .font(.footnote)
                            .accessibilityIdentifier("pair-status")
                    }
                }
            }
            .navigationTitle("Computer")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                        .accessibilityIdentifier("done")
                }
            }
            .sheet(isPresented: $scanning) {
                CodeScannerView(codeTypes: [.qr]) { result in
                    scanning = false
                    if case .success(let code) = result {
                        input = code
                        pair()
                    }
                }
            }
        }
    }

    /// Keep the code once the computer answers to it: it says which sessions
    /// it has open, or that it doesn't offer them.
    private func pair() {
        let code = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard PairedComputer.isPairingCode(code) else { return }
        busy = true
        status = "Reaching \(PairedComputer.address(code))…"
        DispatchQueue.global(qos: .userInitiated).async {
            let result = AgentLink.sessions(code: code)
            DispatchQueue.main.async {
                busy = false
                switch result {
                case .success(let sessions):
                    PairedComputer.save(code)
                    current = code
                    input = ""
                    status = "Paired. \(sessions.count) agent session\(sessions.count == 1 ? " is" : "s are") open there."
                case .failure(let error) where error.reason != nil:
                    // It answered: the code works, sessions or not.
                    PairedComputer.save(code)
                    current = code
                    input = ""
                    status = "Paired. \(error.message)"
                case .failure(let error):
                    status = "Not paired: \(error.message). \(error.advice)"
                }
            }
        }
    }

    private func sync(_ code: String) {
        busy = true
        status = "Syncing…"
        DispatchQueue.global(qos: .userInitiated).async {
            let result = AgentLink.sync(code: code)
            DispatchQueue.main.async {
                busy = false
                switch result {
                case .success(let report):
                    status = "Synced: \(report.notes_pulled) notes in, \(report.notes_pushed) out."
                    AppState.refresh()
                case .failure(let error):
                    status = "Not synced: \(error.message). \(error.advice)"
                }
            }
        }
    }
}
