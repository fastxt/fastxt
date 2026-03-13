//
//  TxtDetailView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/12/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct TxtDetailView: View {
    var note: Note
    @ObservedObject var env = AppState.getEnv()
    @State private var showSummary = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {

            // Header with tags and actions
            HStack {
                Spacer()
                // AI Summarize button
                Button(action: {
                    env.getAiSummary(for: note.txt)
                    showSummary = true
                }) {
                    HStack(spacing: 4) {
                        Image(systemName: "doc.text.magnifyingglass")
                        Text("Summarize")
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Color.blue.opacity(0.1))
                    .foregroundColor(.blue)
                    .cornerRadius(8)
                }
            }

            // Tags row
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(note.tags.split(separator: ","), id: \.self) { tag in
                        Text(tag.trimmingCharacters(in: .whitespaces))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 4)
                            .background(Color.gray.opacity(0.1))
                            .cornerRadius(12)
                            .font(.caption)
                    }
                }
            }

            // Metadata row
            HStack {
                Text(note.created_at.prefix(19))
                    .font(.caption)
                    .foregroundColor(.secondary)
                Spacer()
                Text("\(String(note.uuid4.prefix(5))).. \(String(note.id))")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Divider()

            // AI Summary Section
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
                        .font(.body)
                        .padding()
                        .background(Color.blue.opacity(0.05))
                        .cornerRadius(8)

                    Divider()
                }
                .transition(.opacity)
            }

            // AI Status
            if !env.aiStatus.isEmpty {
                Text(env.aiStatus)
                    .font(.caption)
                    .foregroundColor(.orange)
            }

            // Note content
            ScrollView {
                Text(note.txt)
                    .font(.body)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Spacer()
        }
        .padding()
        .navigationBarTitle("Note Detail", displayMode: .inline)
    }
}

struct TxtDetailView_Previews: PreviewProvider {
    static var previews: some View {
        TxtDetailView(note: Note(
            id: 0,
            uuid4: "uuid4",
            txt: "This is a sample note with some content that demonstrates the text detail view. It includes multiple sentences to show how the summary feature might work.",
            tags: "tag1,tag2",
            created_at: "2020-04-12"
        ))
    }
}
