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

/// Speak a note, fix it up, and keep it as an idea or a prompt.
struct DictateView: View {
    @Environment(\.dismiss) private var dismiss
    @StateObject private var dictation = Dictation()
    /// What was dictated so far, and edited.
    @State private var text = ""
    /// Whether the keyboard is up, fixing the text; the big microphone makes
    /// way for it then, so the save buttons stay in reach.
    @FocusState private var typing: Bool

    var body: some View {
        VStack(spacing: 14) {
            Picker("Language", selection: $dictation.language) {
                ForEach(Dictation.Language.allCases) { language in
                    Text(language.label).tag(language)
                }
            }
            .pickerStyle(.segmented)
            .disabled(dictation.isRecording)

            ZStack(alignment: .topLeading) {
                TextEditor(text: $text)
                    .focused($typing)
                    .disabled(dictation.isRecording)
                    .opacity(dictation.isRecording ? 0 : 1)
                    .accessibilityIdentifier("dictated-text")
                if dictation.isRecording {
                    ScrollView {
                        Text(shown.isEmpty ? "Listening…" : shown)
                            .foregroundColor(shown.isEmpty ? .secondary : .primary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 8)
                    }
                } else if text.isEmpty {
                    Text("Tap the microphone and speak, or type here.")
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 8)
                        .allowsHitTesting(false)
                }
            }
            .frame(minHeight: 180)
            .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.3)))

            if !dictation.problem.isEmpty {
                Text(dictation.problem)
                    .font(.footnote)
                    .foregroundColor(.orange)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            if !typing {
                Button(action: { dictation.toggle() }) {
                    Image(systemName: dictation.isRecording ? "stop.circle.fill" : "mic.circle.fill")
                        .font(.system(size: 72))
                        .foregroundColor(dictation.isRecording ? .red : .accentColor)
                }
                .accessibilityIdentifier("record")
                .accessibilityLabel(dictation.isRecording ? "Stop dictating" : "Dictate")

                Text(dictation.isRecording ? "Listening. Tap to stop." : "Tap to dictate")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                Text("Speech becomes text on this iPhone. The audio is never stored or sent anywhere.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)
            }

            Spacer(minLength: 0)

            HStack(spacing: 12) {
                Button(action: { save(.idea) }) {
                    Text("Save as Idea").frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("save-idea")
                Button(action: { save(.prompt) }) {
                    Text("Save as Prompt").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .accessibilityIdentifier("save-prompt")
            }
            .disabled(trimmed.isEmpty || dictation.isRecording)
        }
        .padding()
        .navigationTitle("Dictate")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") {
                    dictation.cancel()
                    dismiss()
                }
            }
            ToolbarItemGroup(placement: .keyboard) {
                Button(action: {
                    typing = false
                    dictation.start()
                }) {
                    Label("Dictate", systemImage: "mic")
                }
                .accessibilityIdentifier("keyboard-dictate")
                Spacer()
                Button("Done") { typing = false }
                    .accessibilityIdentifier("keyboard-done")
            }
        }
        .onChange(of: dictation.isRecording) { recording in
            if !recording {
                text = joined(text, dictation.takeHeard())
            }
        }
        .onDisappear { dictation.cancel() }
    }

    /// The text with what is being heard now after it.
    private var shown: String {
        joined(text, dictation.heard)
    }

    private var trimmed: String {
        text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func joined(_ before: String, _ heard: String) -> String {
        guard !heard.isEmpty else { return before }
        guard !before.isEmpty else { return heard }
        let separator = before.last?.isWhitespace == true ? "" : dictation.language.joiner
        return before + separator + heard
    }

    private func save(_ kind: NoteKind) {
        AppState.insert(txt: trimmed, tags: kind.tag)
        dismiss()
    }
}

struct DictateView_Previews: PreviewProvider {
    static var previews: some View {
        NavigationStack { DictateView() }
    }
}
