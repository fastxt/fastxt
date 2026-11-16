/*
 Fastxt
 Copyright (C) 2020  Yi Wang
 
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
//
//  AppState.swift
//  localnative-ios
//
//  Created by Yi Wang on 4/5/20.
//

import Foundation

class AppState {
    static let LIMIT :Int64 = 50
    static var offset :Int64 = 0
    static var count :Int64 = 0
    static var query = ""
    @discardableResult
    static func makePaginationText()-> String {
        let start = count > 0 ? offset + 1 : 0
        let end : Int64 = offset + LIMIT > count ? count : offset + LIMIT
        
//        let p: Int64 = Int64(ceil((0.0 + Double(end)) / Double(LIMIT)))
//        let z: Int64 = Int64(ceil((Double(count) + 0.0) / Double(LIMIT)))
        env.paginationText = "\(start)-\(end)/\(count)"
        return "\(start)-\(end)/\(count)"
    }
    static func getQuery() -> String{
        return query
    }
    static func setQuery(query: String) {
        self.query = query
    }
    static func setCount(count: Int64) {
        self.count = count
    }

    static func incOffset()-> Int64 {
        if(offset + LIMIT < count) {
            offset += LIMIT
        }
        return offset
    }
    static func decOffset()-> Int64 {
        if(offset - LIMIT >= 0) {
            offset -= LIMIT
        }
        return offset
    }
    static func clearOffset() {
        offset = 0
    }
    
    static func getOffset() -> Int64 {
        return offset
    }
    static func setOffset(offset: Int64) {
        self.offset = offset
    }
    static let env = Env()
    static func getEnv()->Env{
        return env
    }
    
    static let ft = RustFastxt()
    static func search(input: String, offset: Int64) {
        AppState.setOffset(offset: offset)
        AppState.setQuery(query: input)
        loadSentMarks()
        if env.filter == .sent {
            loadSentNotes(matching: input)
            return
        }
        let encoder = JSONEncoder()
        let cmd = CmdSearch(
            action: "search",
            query: input,
            limit: LIMIT,
            offset: offset,
            tags: env.filter.tag.map { [$0] }
        )
        do {
            let data = try encoder.encode(cmd)
            let input = String(data: data, encoding: .utf8)!
            let txt = AppState.ft.run(json_input: input)
            let data1 = txt.data(using: .utf8)!
            let decoder = JSONDecoder()
            do {
                let resp = try decoder.decode(Response.self, from: data1)
                AppState.setCount(count: resp.count)
                AppState.env.notes = resp.notes
            } catch {
                print(error.localizedDescription)
            }
            makePaginationText()
        } catch {
            print(error.localizedDescription)
        }

    }

    /// The list again, from the top, as it now is.
    static func refresh() {
        search(input: getQuery(), offset: 0)
    }

    /// Which notes were sent somewhere, to mark them in the list.
    static func loadSentMarks() {
        env.sentNoteUUIDs = Set(AgentLink.sent(limit: 1000).map { $0.note_uuid })
    }

    /// The notes that were sent, the one sent last first.
    private static func loadSentNotes(matching query: String) {
        var seen = Set<String>()
        var notes: [Note] = []
        for record in AgentLink.sent(limit: 1000) where seen.insert(record.note_uuid).inserted {
            if let note = getNote(uuid4: record.note_uuid),
               query.isEmpty || note.txt.localizedCaseInsensitiveContains(query) {
                notes.append(note)
            }
        }
        setCount(count: Int64(notes.count))
        env.notes = notes
        makePaginationText()
    }

    static func getNote(uuid4: String) -> Note? {
        run(["action": "get", "uuid4": uuid4], field: "note", as: Note.self)
    }

    /// Change a note's text and tags; returns it as it now is.
    @discardableResult
    static func update(note: Note, txt: String, tags: String) -> Note? {
        run(["action": "update", "rowid": note.id, "txt": txt, "tags": tags], field: "note", as: Note.self)
    }

    /// Make a note an idea or a prompt, or neither.
    @discardableResult
    static func setKind(_ kind: NoteKind?, of note: Note) -> Note? {
        update(note: note, txt: note.txt, tags: NoteKind.set(kind, in: note.tags))
    }

    private static func run<T: Decodable>(_ command: [String: Any], field: String, as type: T.Type) -> T? {
        try? AgentLink.call(command, field: field, as: type).get()
    }
    
    static func insert(txt:String, tags:String){
        let encoder = JSONEncoder()
        let cmd = CmdInsert(
            action: "insert",
            txt: txt,
            tags: tags,
            limit: 10,
            offset: 0
        )
        do {
            let data = try encoder.encode(cmd)
            let input = String(data: data, encoding: .utf8)!
            AppState.ft.run(json_input: input)
        } catch {
            print(error.localizedDescription)
        }
        // Show the new note: take away what would hide it.
        let kind = NoteKind.of(tags: tags)
        switch env.filter {
        case .ideas where kind != .idea, .prompts where kind != .prompt, .sent:
            env.filter = .all
        default:
            break
        }
        if !env.searchText.isEmpty {
            env.searchText = ""
        }
        refresh()
    }
    
    static func delete(rowid: Int64){
        let encoder = JSONEncoder()
        let cmd = CmdDelete(
            action: "delete",
            query: AppState.getQuery(),
            rowid: rowid,
            limit: 10,
            offset: AppState.getOffset()
        )
        do {
            let data = try encoder.encode(cmd)
            let input = String(data: data, encoding: .utf8)!
            print(input)
            AppState.ft.run(json_input: input)
        } catch {
            print(error.localizedDescription)
        }
    }
}

struct Note: Codable, Identifiable, Hashable {
    var id: Int64
    var uuid4: String
    var txt: String
    var tags: String
    var created_at: String
    private enum CodingKeys: String, CodingKey {
        case id = "rowid"
        case uuid4
        case txt
        case tags
        case created_at
    }
}

/// The core's time stamps (`created_at`, `sent_at`) are UTC, written
/// `yyyy-MM-dd HH:mm:ss`; people read them in this phone's time zone.
enum Stamp {
    private static let parser: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(identifier: "UTC")
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return formatter
    }()

    private static let shown: DateFormatter = {
        let formatter = DateFormatter()
        formatter.dateStyle = .medium
        formatter.timeStyle = .short
        formatter.doesRelativeDateFormatting = true
        return formatter
    }()

    static func date(_ utc: String) -> Date? {
        parser.date(from: String(utc.prefix(19)))
    }

    /// "Today at 10:03 AM", in this phone's time zone; the stamp as it is
    /// when it can't be read.
    static func local(_ utc: String) -> String {
        date(utc).map(shown.string(from:)) ?? utc
    }
}

struct CmdInsert: Codable {
    var action: String
    var txt: String
    var tags: String
    var limit: Int64
    var offset: Int64
}

struct CmdSearch: Codable {
    var action: String
    var query: String
    var limit: Int64
    var offset: Int64
    /// Tags a note must carry, such as `kind:prompt`.
    var tags: [String]?
}

struct CmdDelete: Codable {
    var action: String
    var query: String
    var rowid: Int64
    var limit: Int64
    var offset: Int64
}

// AI Command structs
struct CmdAiTag: Codable {
    var action: String
    var text: String
    var endpoint: String?
    var model: String?
}

struct CmdAiSummarize: Codable {
    var action: String
    var rowid: Int64
    var endpoint: String?
    var model: String?
}

struct Response: Decodable {
    let count: Int64
    let notes: [Note]
}


class Env: ObservableObject {
    @Published var addr:String = ""
    @Published var notes:[Note] = []
    @Published var paginationText:String = "/"
    @Published var searchText = "" {
        didSet {
            AppState.search(input: searchText, offset: 0)
        }
    }
    /// Which notes the list shows.
    @Published var filter: NoteFilter = .all {
        didSet {
            AppState.search(input: searchText, offset: 0)
        }
    }
    /// The notes that were sent to a coding session.
    @Published var sentNoteUUIDs: Set<String> = []
    @Published var isServerRunning:Bool = false

    // AI-related state
    @Published var aiSuggestedTags: [String] = []
    @Published var aiSummary: String = ""
    @Published var aiStatus: String = ""
    @Published var aiAvailable: Bool = false
    @Published var showAiTags: Bool = false

    /// Get AI tag suggestions for text using Apple Foundation Models.
    func getAiTags(for text: String) {
        guard !text.isEmpty else {
            aiStatus = "Enter some text first"
            return
        }

        aiStatus = "Getting AI suggestions..."

        // Off the main thread: Foundation Models can take seconds.
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let response = FastxtAI.suggestTags(text: text)
            if let data = response.data(using: .utf8),
               let result = try? JSONDecoder().decode(AiTagsResponse.self, from: data) {
                DispatchQueue.main.async {
                    self?.aiSuggestedTags = result.tags
                    self?.aiAvailable = result.available
                    self?.aiStatus = result.error ?? ""
                    self?.showAiTags = !result.tags.isEmpty
                }
            }
        }
    }

    /// Use suggested AI tags.
    func useAiTags(currentTags: String) -> String {
        let newTags = aiSuggestedTags.joined(separator: ",")
        let result = currentTags.isEmpty ? newTags : "\(currentTags),\(newTags)"
        clearAiTags()
        return result
    }

    /// Clear AI suggestions.
    func clearAiTags() {
        aiSuggestedTags = []
        showAiTags = false
        aiStatus = ""
    }

    /// Get AI summary for text.
    func getAiSummary(for text: String) {
        guard !text.isEmpty else {
            aiStatus = "Enter some text first"
            return
        }

        aiStatus = "Generating summary..."

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            let response = FastxtAI.summarize(text: text)
            if let data = response.data(using: .utf8),
               let result = try? JSONDecoder().decode(AiSummaryResponse.self, from: data) {
                DispatchQueue.main.async {
                    self?.aiSummary = result.summary ?? ""
                    self?.aiAvailable = result.available
                    self?.aiStatus = result.error ?? ""
                }
            }
        }
    }

    /// Check if AI is available.
    func checkAiAvailability() {
        aiAvailable = FastxtAI.isAvailable()
    }
}

extension DispatchQueue {

    static func background(delay: Double = 0.0, background: (()->Void)? = nil, completion: (() -> Void)? = nil) {
        DispatchQueue.global(qos: .background).async {
            background?()
            if let completion = completion {
                DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: {
                    completion()
                })
            }
        }
    }

}
