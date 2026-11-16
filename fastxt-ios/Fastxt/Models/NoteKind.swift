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

import Foundation

/// What a note is for: a raw idea, or a prompt ready to send to a coding
/// agent. Kept as a tag on the note (`kind:idea`, `kind:prompt`), so it syncs
/// with it and every Fastxt app can filter by it.
enum NoteKind: String, CaseIterable, Identifiable {
    case idea
    case prompt

    var id: String { rawValue }

    var tag: String { "kind:\(rawValue)" }

    var label: String {
        switch self {
        case .idea: return "Idea"
        case .prompt: return "Prompt"
        }
    }

    /// The tags in a stored tag string, split as the core splits them:
    /// commas (ASCII or full-width) and newlines, inner spaces collapsed.
    static func split(_ tags: String) -> [String] {
        tags.components(separatedBy: CharacterSet(charactersIn: ",，\n\r"))
            .map { $0.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ") }
            .filter { !$0.isEmpty }
    }

    private static func isKindTag(_ tag: String) -> Bool {
        tag.lowercased().hasPrefix("kind:")
    }

    /// The kind a note's tags give it, if any.
    static func of(tags: String) -> NoteKind? {
        for tag in split(tags) where isKindTag(tag) {
            if let kind = NoteKind(rawValue: String(tag.lowercased().dropFirst("kind:".count))) {
                return kind
            }
        }
        return nil
    }

    /// The tag string with this kind, in place of any other; `nil` takes it
    /// away.
    static func set(_ kind: NoteKind?, in tags: String) -> String {
        var kept = split(tags).filter { !isKindTag($0) }
        if let kind = kind {
            kept.insert(kind.tag, at: 0)
        }
        return kept.joined(separator: ",")
    }

    /// The tags people see; the kind is shown on its own.
    static func visibleTags(_ tags: String) -> [String] {
        split(tags).filter { !isKindTag($0) }
    }
}

/// Which notes the list shows.
enum NoteFilter: String, CaseIterable, Identifiable {
    case all
    case ideas
    case prompts
    case sent

    var id: String { rawValue }

    var label: String {
        switch self {
        case .all: return "All"
        case .ideas: return "Ideas"
        case .prompts: return "Prompts"
        case .sent: return "Sent"
        }
    }

    /// The tag a note must carry to be listed.
    var tag: String? {
        switch self {
        case .ideas: return NoteKind.idea.tag
        case .prompts: return NoteKind.prompt.tag
        case .all, .sent: return nil
        }
    }
}
