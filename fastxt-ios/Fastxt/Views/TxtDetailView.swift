//
//  TxtDetailView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/12/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct TxtDetailView: View {
    @State var note: Note
    @ObservedObject var env = AppState.getEnv()
    @Environment(\.dismiss) private var dismiss
    @State private var editing = false
    @State private var draft = ""
    @State private var showingSend = false
    @State private var sentRecords: [SentRecord] = []
    @State private var showSummary = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Picker("Kind", selection: kind) {
                    Text("Note").tag(NoteKind?.none)
                    ForEach(NoteKind.allCases) { kind in
                        Text(kind.label).tag(NoteKind?.some(kind))
                    }
                }
                .pickerStyle(.segmented)
                .disabled(editing)
                .accessibilityIdentifier("kind")

                if editing {
                    TextEditor(text: $draft)
                        .frame(minHeight: 220)
                        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.3)))
                } else {
                    Text(note.txt)
                        .font(.body)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }

                let tags = NoteKind.visibleTags(note.tags)
                if !tags.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(tags, id: \.self) { tag in
                                // A tag finds the other notes that have it.
                                Button(action: { search(tag) }) {
                                    Text(tag)
                                        .padding(.horizontal, 10)
                                        .padding(.vertical, 4)
                                        .background(Color.gray.opacity(0.1))
                                        .cornerRadius(12)
                                        .font(.caption)
                                }
                                .buttonStyle(.plain)
                                .disabled(editing)
                            }
                        }
                    }
                }

                Text(Stamp.local(note.created_at))
                    .font(.caption)
                    .foregroundColor(.secondary)

                Button(action: { showingSend = true }) {
                    Label("Send to a coding session", systemImage: "paperplane")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(editing)
                .accessibilityIdentifier("send")

                if !sentRecords.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Sent")
                            .font(.headline)
                        ForEach(sentRecords) { record in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(record.title)
                                    .font(.subheadline)
                                Text("\((record.cwd as NSString).lastPathComponent) · \(Stamp.local(record.sent_at))\(record.started ? "" : " · not seen starting")")
                                    .font(.caption)
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                    .accessibilityIdentifier("sent-records")
                }

                if showSummary && !env.aiSummary.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        HStack {
                            Text("AI Summary")
                                .font(.headline)
                                .foregroundColor(.blue)
                            Spacer()
                            Button(action: {
                                showSummary = false
                                env.aiSummary = ""
                            }) {
                                Image(systemName: "xmark.circle.fill")
                                    .foregroundColor(.gray)
                            }
                        }
                        Text(env.aiSummary)
                            .padding()
                            .background(Color.blue.opacity(0.05))
                            .cornerRadius(8)
                    }
                }
                if !env.aiStatus.isEmpty {
                    Text(env.aiStatus)
                        .font(.caption)
                        .foregroundColor(.orange)
                }
            }
            .padding()
        }
        .navigationTitle(NoteKind.of(tags: note.tags)?.label ?? "Note")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if editing {
                    Button("Save") { saveEdit() }
                        .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                } else {
                    Menu {
                        Button(action: {
                            draft = note.txt
                            editing = true
                        }) {
                            Label("Edit", systemImage: "pencil")
                        }
                        Button(action: {
                            env.getAiSummary(for: note.txt)
                            showSummary = true
                        }) {
                            Label("Summarize", systemImage: "doc.text.magnifyingglass")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
            }
        }
        .sheet(isPresented: $showingSend, onDismiss: reloadSent) {
            SendSheet(note: note)
        }
        .onAppear(perform: reloadSent)
    }

    /// The note's kind; changing it saves it.
    private var kind: Binding<NoteKind?> {
        Binding(
            get: { NoteKind.of(tags: note.tags) },
            set: { kind in
                if let updated = AppState.setKind(kind, of: note) {
                    note = updated
                    AppState.refresh()
                }
            }
        )
    }

    private func saveEdit() {
        if let updated = AppState.update(note: note, txt: draft, tags: note.tags) {
            note = updated
            AppState.refresh()
        }
        editing = false
    }

    private func reloadSent() {
        sentRecords = AgentLink.sent(noteUUID: note.uuid4)
    }

    private func search(_ tag: String) {
        env.searchText = tag
        dismiss()
    }
}

struct TxtDetailView_Previews: PreviewProvider {
    static var previews: some View {
        NavigationStack {
            TxtDetailView(note: Note(
                id: 0,
                uuid4: "uuid4",
                txt: "Fix the flaky login test, and add a test for the retry path.",
                tags: "kind:prompt,auth",
                created_at: "2026-11-16 10:12:00"
            ))
        }
    }
}
