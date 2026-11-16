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

/// Pick one of the coding-agent sessions open on the paired computer and
/// type the note into it, as if you had typed it there.
struct SendSheet: View {
    let note: Note
    @Environment(\.dismiss) private var dismiss
    @State private var code = PairedComputer.load()
    @State private var sessions: [AgentSession] = []
    @State private var loading = false
    @State private var sendingTo: String?
    @State private var failure: LinkError?
    @State private var done: (SentRecord, AgentSession)?
    @State private var pairing = false

    var body: some View {
        NavigationStack {
            List {
                Section(header: Text("Sending")) {
                    Text(note.txt)
                        .font(.callout)
                        .lineLimit(5)
                }
                if let done = done {
                    Section {
                        Label(result(done.0, done.1), systemImage: "checkmark.circle.fill")
                            .foregroundColor(.green)
                            .accessibilityIdentifier("send-result")
                    }
                } else if code == nil {
                    Section(footer: Text("On your computer run `fastxt serve --agents`, then pair with the code it shows.")) {
                        Button("Pair with your computer") { pairing = true }
                            .accessibilityIdentifier("pair-computer")
                    }
                } else {
                    if let failure = failure {
                        Section {
                            Text(failure.message)
                                .foregroundColor(failure.reason == "not-an-agent" ? .red : .orange)
                            if !failure.advice.isEmpty {
                                Text(failure.advice)
                                    .font(.footnote)
                                    .foregroundColor(.secondary)
                            }
                            if failure.reason == nil || failure.reason == "unavailable" {
                                Button("Pair again") { pairing = true }
                            }
                        }
                    }
                    if loading {
                        Section { ProgressView("Asking your computer…") }
                    } else if sessions.isEmpty && failure == nil {
                        Section {
                            Text("No agent session is open in Orca on your computer.")
                                .foregroundColor(.secondary)
                        }
                    }
                    ForEach(folders, id: \.self) { folder in
                        Section(header: Text(folder)) {
                            ForEach(sessions.filter { $0.folderName == folder }) { session in
                                row(session)
                            }
                        }
                    }
                }
            }
            .navigationTitle("Send to a session")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(done == nil ? "Cancel" : "Done") { dismiss() }
                        .accessibilityIdentifier("close-send")
                }
                ToolbarItem(placement: .primaryAction) {
                    Button(action: load) { Image(systemName: "arrow.clockwise") }
                        .disabled(code == nil || loading || sendingTo != nil)
                        .accessibilityLabel("Refresh")
                }
            }
            .sheet(isPresented: $pairing, onDismiss: {
                code = PairedComputer.load()
                load()
            }) {
                PairView()
            }
            .onAppear(perform: load)
        }
    }

    /// The project folders, in the order their newest session comes.
    private var folders: [String] {
        var seen: [String] = []
        for session in sessions where !seen.contains(session.folderName) {
            seen.append(session.folderName)
        }
        return seen
    }

    @ViewBuilder
    private func row(_ session: AgentSession) -> some View {
        Button(action: { send(to: session) }) {
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(session.agent)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Color.accentColor.opacity(0.12))
                        .cornerRadius(5)
                    Text(session.name.isEmpty ? "Untitled session" : session.name)
                        .foregroundColor(.primary)
                        .lineLimit(1)
                    Spacer()
                    if sendingTo == session.handle {
                        ProgressView()
                    } else {
                        StateBadge(state: session.state)
                    }
                }
                if session.isAsking {
                    Text("Asking you something. Answer it on the computer first.")
                        .font(.caption)
                        .foregroundColor(.orange)
                } else if let last = session.last_prompt, !last.isEmpty {
                    Text(last)
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .lineLimit(1)
                }
                if let branch = session.branch {
                    Text(branch)
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }
            }
        }
        .disabled(session.isAsking || sendingTo != nil)
        .accessibilityIdentifier("session-\(session.handle)")
    }

    private func load() {
        guard let code = code else { return }
        loading = true
        failure = nil
        DispatchQueue.global(qos: .userInitiated).async {
            let result = AgentLink.sessions(code: code)
            DispatchQueue.main.async {
                loading = false
                switch result {
                case .success(let list):
                    sessions = list
                case .failure(let error):
                    sessions = []
                    failure = error
                }
            }
        }
    }

    private func send(to session: AgentSession) {
        guard let code = code else { return }
        sendingTo = session.handle
        failure = nil
        let rowid = note.id
        DispatchQueue.global(qos: .userInitiated).async {
            let result = AgentLink.send(code: code, rowid: rowid, handle: session.handle)
            DispatchQueue.main.async {
                sendingTo = nil
                switch result {
                case .success(let sent):
                    done = (sent, session)
                    AppState.refresh()
                case .failure(let error):
                    failure = error
                    if error.reason == "gone" || error.reason == "asking" {
                        load()
                    }
                }
            }
        }
    }

    private func result(_ sent: SentRecord, _ session: AgentSession) -> String {
        let place = "\(sent.agent) · \(sent.session.isEmpty ? session.folderName : sent.session)"
        if sent.started {
            return "Typed into \(place). It started on it."
        }
        if session.state == "working" {
            return "Typed into \(place). It was working, so the prompt waits behind what it is doing."
        }
        return "Typed into \(place). It hasn't been seen starting on it yet; look at the session."
    }
}

/// A session's state, as a word with a colour.
struct StateBadge: View {
    let state: String

    var body: some View {
        if !state.isEmpty {
            Text(state)
                .font(.caption2.weight(.medium))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .foregroundColor(color)
                .background(color.opacity(0.12))
                .cornerRadius(5)
        }
    }

    private var color: Color {
        switch state {
        case "ready": return .green
        case "working": return .blue
        case "asking": return .orange
        default: return .secondary
        }
    }
}
