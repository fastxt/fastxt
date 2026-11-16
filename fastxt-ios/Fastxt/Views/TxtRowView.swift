//
//  TxtRowView.swift
//  Fastxt
//
//  Created by Yi Wang on 3/29/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct TxtRowView: View {
    var note: Note
    /// Whether it was sent to a coding session.
    var sent: Bool = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                if let kind = NoteKind.of(tags: note.tags) {
                    KindBadge(kind: kind)
                }
                if sent {
                    Label("Sent", systemImage: "paperplane.fill")
                        .font(.caption2)
                        .foregroundColor(.green)
                }
                Spacer()
                Text(Stamp.local(note.created_at))
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Text(note.txt.trunc(length: 280))
                .lineLimit(3)
            let tags = NoteKind.visibleTags(note.tags)
            if !tags.isEmpty {
                Text(tags.joined(separator: " · "))
                    .font(.caption)
                    .foregroundColor(.blue)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }
}

/// Idea or Prompt, as a small label.
struct KindBadge: View {
    let kind: NoteKind

    var body: some View {
        Text(kind.label)
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundColor(kind == .prompt ? .white : .purple)
            .background(kind == .prompt ? Color.purple : Color.purple.opacity(0.12))
            .cornerRadius(5)
    }
}

struct TxtRowView_Previews: PreviewProvider {
    static var previews: some View {
        TxtRowView(note: Note(
            id: 0,
            uuid4: "uuid4",
            txt: "Fix the flaky login test, and add a test for the retry path.",
            tags: "kind:prompt,auth",
            created_at: "2026-11-16 10:12:00"
        ), sent: true)
    }
}

extension String {
  /*
   Truncates the string to the specified length number of characters and appends an optional trailing string if longer.
   - Parameter length: Desired maximum lengths of a string
   - Parameter trailing: A 'String' that will be appended after the truncation.

   - Returns: 'String' object.
  */
  func trunc(length: Int, trailing: String = "…") -> String {
    return (self.count > length) ? self.prefix(length) + trailing : self
  }
}
