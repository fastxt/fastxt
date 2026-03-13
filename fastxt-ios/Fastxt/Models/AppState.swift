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
    static let LIMIT :Int64 = 10
    static var offset :Int64 = 0
    static var count :Int64 = 0
    static var query = ""
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
        let encoder = JSONEncoder()
        let cmd = CmdSearch(
            action: "search",
            query: input,
            limit: 10,
            offset: offset
        )
        do {
            let data = try encoder.encode(cmd)
            let input = String(data: data, encoding: .utf8)!
            print(input)
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
            print(input)
            AppState.ft.run(json_input: input)
            AppState.search(input: "", offset: 0)
        } catch {
            print(error.localizedDescription)
        }
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

// AI Response structs
struct AiTagsResponse: Decodable {
    let tags: [String]
    let available: Bool
    let error: String?
}

struct AiSummaryResponse: Decodable {
    let summary: String?
    let available: Bool
    let error: String?
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

        // Use Apple Foundation Models directly
        let response = FastxtAI.suggestTags(text: text)

        if let data = response.data(using: .utf8),
           let result = try? JSONDecoder().decode(AiTagsResponse.self, from: data) {
            DispatchQueue.main.async { [weak self] in
                self?.aiSuggestedTags = result.tags
                self?.aiAvailable = result.available
                self?.aiStatus = result.error ?? ""
                self?.showAiTags = !result.tags.isEmpty
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

        let response = FastxtAI.summarize(text: text)

        if let data = response.data(using: .utf8),
           let result = try? JSONDecoder().decode(AiSummaryResponse.self, from: data) {
            DispatchQueue.main.async { [weak self] in
                self?.aiSummary = result.summary ?? ""
                self?.aiAvailable = result.available
                self?.aiStatus = result.error ?? ""
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
