//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var env : Env
    @State private var showingDictate = false
    @State private var showingCreate = false
    @State private var showingComputer = false
    @State private var showingSync = false
    @State private var showingCategories = false
    @State private var pendingDelete: Note?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Picker("Show", selection: $env.filter) {
                    ForEach(NoteFilter.allCases) { filter in
                        Text(filter.label).tag(filter)
                    }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal)
                .padding(.bottom, 6)
                .accessibilityIdentifier("filter")

                List {
                    HStack {
                        Image(systemName: "magnifyingglass").foregroundColor(.secondary)
                        TextField("Search", text: $env.searchText)
                            .autocorrectionDisabled()
                        if !env.searchText.isEmpty {
                            Button("Clear") { env.searchText = "" }
                                .buttonStyle(.borderless)
                        }
                    }
                    if env.notes.isEmpty {
                        Text(emptyText)
                            .foregroundColor(.secondary)
                    }
                    ForEach(env.notes) { note in
                        NavigationLink(value: note) {
                            TxtRowView(note: note, sent: env.sentNoteUUIDs.contains(note.uuid4))
                        }
                        .swipeActions {
                            // Not the destructive role: that removes the row
                            // before the deletion is confirmed.
                            Button { pendingDelete = note } label: {
                                Label("Delete", systemImage: "trash")
                            }
                            .tint(.red)
                        }
                    }
                    if AppState.count > AppState.LIMIT && env.filter != .sent {
                        HStack {
                            Button("Previous") {
                                AppState.search(input: AppState.getQuery(), offset: AppState.decOffset())
                            }
                            .buttonStyle(.borderless)
                            Spacer()
                            Text(env.paginationText).foregroundColor(.secondary)
                            Spacer()
                            Button("Next") {
                                AppState.search(input: AppState.getQuery(), offset: AppState.incOffset())
                            }
                            .buttonStyle(.borderless)
                        }
                    }
                }
                .listStyle(.plain)

                HStack(spacing: 12) {
                    Button(action: { showingCreate = true }) {
                        Label("Type", systemImage: "square.and.pencil")
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("type")
                    Button(action: { showingDictate = true }) {
                        Label("Dictate", systemImage: "mic.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .accessibilityIdentifier("dictate")
                }
                .padding()
            }
            .navigationTitle("Fastxt")
            .navigationDestination(for: Note.self) { note in
                TxtDetailView(note: note)
            }
            .navigationDestination(isPresented: $showingSync) { SyncView() }
            .navigationDestination(isPresented: $showingCategories) { CategoryView() }
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Menu {
                        Button(action: { showingSync = true }) {
                            Label("Sync with another device", systemImage: "arrow.triangle.2.circlepath")
                        }
                        Button(action: { showingCategories = true }) {
                            Label("Categories", systemImage: "folder")
                        }
                    } label: {
                        Image(systemName: "ellipsis.circle")
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: { showingComputer = true }) {
                        Image(systemName: "desktopcomputer")
                    }
                    .accessibilityLabel("Computer")
                    .accessibilityIdentifier("computer")
                }
            }
            .sheet(isPresented: $showingDictate, onDismiss: AppState.refresh) {
                NavigationStack { DictateView() }
            }
            .sheet(isPresented: $showingCreate, onDismiss: AppState.refresh) {
                NavigationStack {
                    CreateView(isPresented: $showingCreate)
                        .navigationTitle("Type a note")
                        .navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .cancellationAction) {
                                Button("Cancel") { showingCreate = false }
                            }
                        }
                }
            }
            .sheet(isPresented: $showingComputer, onDismiss: AppState.refresh) { PairView() }
            .confirmationDialog(
                "Delete this note?",
                isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } }),
                titleVisibility: .visible,
                presenting: pendingDelete
            ) { note in
                Button("Delete", role: .destructive) {
                    AppState.delete(rowid: note.id)
                    AppState.refresh()
                }
            } message: { _ in
                Text("It is deleted on every device it syncs with. There is no undo.")
            }
        }
    }

    private var emptyText: String {
        if !env.searchText.isEmpty {
            return "No note matches."
        }
        switch env.filter {
        case .all: return "No notes yet. Tap Dictate to speak one."
        case .ideas: return "No ideas yet."
        case .prompts: return "No prompts yet. Save a note as a prompt to send it to a coding session."
        case .sent: return "Nothing sent yet."
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView().environmentObject(AppState.getEnv())
    }
}
