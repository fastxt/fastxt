//
//  CategoryView.swift
//  Fastxt
//
//  Created by AI Assistant
//  Copyright © 2024 Yi Wang. All rights reserved.
//

import SwiftUI

/// View showing notes grouped by AI-generated categories.
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

        // Get all notes
        let input = "{\"action\":\"select\",\"limit\":500,\"offset\":0}"
        let result = fastxt_run(input)

        if let data = result?.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
           let notesArray = json["notes"] as? [[String: Any]] {

            var grouped: [String: [Note]] = [:]

            for noteDict in notesArray {
                if let txt = noteDict["txt"] as? String {
                    let note = Note(
                        rowid: noteDict["rowid"] as? Int64 ?? 0,
                        txt: txt,
                        tags: noteDict["tags"] as? String ?? "",
                        aiCategory: noteDict["ai_category"] as? String
                    )

                    let category = note.aiCategory?.isEmpty == false ? note.aiCategory! : "uncategorized"
                    if grouped[category] == nil {
                        grouped[category] = []
                    }
                    grouped[category]?.append(note)
                }
            }

            categories = grouped
        }

        isLoading = false
    }

    /// Organize notes using AI categorization.
    private func organizeNotes() {
        organizeStatus = "Organizing..."
        isLoading = true

        // Call ai-organize command
        let input = "{\"action\":\"ai-organize\",\"limit\":100}"
        let result = fastxt_run(input)

        if let data = result?.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {

            if let processed = json["processed"] as? Int {
                let errors = json["errors"] as? Int ?? 0
                organizeStatus = "Organized \(processed) notes (\(errors) errors)"
            } else if let error = json["error"] as? String {
                organizeStatus = "Error: \(error)"
            }
        } else {
            organizeStatus = "Failed to organize"
        }

        // Reload categories
        loadCategories()
    }
}

/// Note model for category view.
struct Note: Hashable {
    let rowid: Int64
    let txt: String
    let tags: String
    let aiCategory: String?

    func hash(into hasher: inout Hasher) {
        hasher.combine(rowid)
    }
}
