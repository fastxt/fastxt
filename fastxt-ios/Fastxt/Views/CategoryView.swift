//
//  CategoryView.swift
//  Fastxt
//
//  View showing notes grouped by AI-generated categories.
//

import SwiftUI

/// A note with its AI category, as read from the select response.
private struct CategorizedNote: Hashable {
    let note: Note
    let category: String?
}

struct CategoryView: View {
    @EnvironmentObject var env: Env
    @State private var categories: [String: [Note]] = [:]
    @State private var isLoading = false
    @State private var organizeStatus = ""

    var body: some View {
        NavigationView {
            VStack {
                if isLoading {
                    ProgressView("Loading categories...")
                } else if categories.isEmpty {
                    VStack(spacing: 20) {
                        Image(systemName: "folder.badge.questionmark")
                            .font(.system(size: 60))
                            .foregroundColor(.gray)
                        Text("No categories yet")
                            .font(.headline)
                        Text("Run 'Organize Notes' to categorize your notes using AI")
                            .font(.subheadline)
                            .foregroundColor(.gray)
                            .multilineTextAlignment(.center)
                        Button(action: organizeNotes) {
                            Label("Organize Notes", systemImage: "sparkles")
                                .padding()
                                .background(Color.blue)
                                .foregroundColor(.white)
                                .cornerRadius(10)
                        }
                        if !organizeStatus.isEmpty {
                            Text(organizeStatus)
                                .font(.caption)
                                .foregroundColor(.gray)
                        }
                    }
                    .padding()
                } else {
                    List {
                        ForEach(sortedCategories(), id: \.self) { category in
                            Section(header:
                                HStack {
                                    Image(systemName: categoryIcon(for: category))
                                    Text("\(category.capitalized) (\(categories[category]?.count ?? 0))")
                                }
                            ) {
                                ForEach(categories[category] ?? [], id: \.self) { note in
                                    NavigationLink(destination: TxtDetailView(note: note)) {
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text(note.txt.prefix(80))
                                                .font(.body)
                                                .lineLimit(2)
                                            if !note.tags.isEmpty {
                                                Text(note.tags)
                                                    .font(.caption)
                                                    .foregroundColor(.blue)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    .listStyle(GroupedListStyle())
                }
            }
            .navigationBarTitle("Categories", displayMode: .inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(action: organizeNotes) {
                        Image(systemName: "arrow.clockwise")
                    }
                }
            }
        }
        .onAppear {
            loadCategories()
        }
    }

    /// Sort categories alphabetically, with "uncategorized" at the end.
    private func sortedCategories() -> [String] {
        var cats = Array(categories.keys)
        cats.sort { a, b in
            if a == "uncategorized" { return false }
            if b == "uncategorized" { return true }
            return a < b
        }
        return cats
    }

    /// Get icon for category.
    private func categoryIcon(for category: String) -> String {
        switch category.lowercased() {
        case "work":
            return "briefcase"
        case "personal":
            return "person"
        case "reference":
            return "book"
        case "idea":
            return "lightbulb"
        case "task":
            return "checkmark.circle"
        case "other":
            return "ellipsis.circle"
        default:
            return "folder"
        }
    }

    /// Load notes grouped by category.
    private func loadCategories() {
        isLoading = true
        DispatchQueue.global(qos: .userInitiated).async {
            let response = AppState.ft.run(json_input: #"{"action":"select","limit":500,"offset":0}"#)
            var grouped: [String: [Note]] = [:]

            if let data = response.data(using: .utf8),
               let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
               let notesArray = json["notes"] as? [[String: Any]] {

                for noteDict in notesArray {
                    guard let txt = noteDict["txt"] as? String else { continue }
                    let note = Note(
                        id: noteDict["rowid"] as? Int64 ?? Int64(noteDict["rowid"] as? Int ?? 0),
                        uuid4: noteDict["uuid4"] as? String ?? "",
                        txt: txt,
                        tags: noteDict["tags"] as? String ?? "",
                        created_at: noteDict["created_at"] as? String ?? ""
                    )
                    let category = (noteDict["ai_category"] as? String)
                        .flatMap { $0.isEmpty ? nil : $0 } ?? "uncategorized"
                    grouped[category, default: []].append(note)
                }
            }

            DispatchQueue.main.async {
                self.categories = grouped
                self.isLoading = false
            }
        }
    }

    /// Organize notes using AI categorization.
    private func organizeNotes() {
        organizeStatus = "Organizing..."
        isLoading = true
        DispatchQueue.global(qos: .userInitiated).async {
            let response = AppState.ft.run(json_input: #"{"action":"ai-organize","limit":100}"#)
            var status = "Failed to organize"
            if let data = response.data(using: .utf8),
               let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                if let processed = json["processed"] as? Int {
                    let errors = json["errors"] as? Int ?? 0
                    status = "Organized \(processed) notes (\(errors) errors)"
                } else if let error = json["error"] as? String {
                    status = "Error: \(error)"
                }
            }
            DispatchQueue.main.async {
                self.organizeStatus = status
                self.isLoading = false
                self.loadCategories()
            }
        }
    }
}
