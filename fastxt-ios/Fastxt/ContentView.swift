//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct Note: Codable, Identifiable {
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

struct Response: Decodable {
    let count: Int64
    let notes: [Note]
}

struct ContentView: View {
    @State private var searchText : String = ""
    @State var notes : [Note] = []

    var body: some View {
        NavigationView {
            VStack {
                SearchBar(text: $searchText, notes: $notes, placeholder: "type to search")
                List (notes){
                        note in
                        Text(note.txt)
                }.navigationBarTitle(Text("Fastxt"))
                Button(action:{
                    let ft = RustFastxt()
                    let txt = "txt"
                    let tags = "tags"
                    ft.run(json_input:"""
                        {"action":"insert",
                        "txt":"\(txt)",
                        "tags":"\(tags)",
                        "limit": 10,
                        "offset": 0
                        }
                        """
                    )
                    self.notes.append(Note(
                        id: 1000,
                        //rowid: 1000,
                        uuid4: "uuid4",
                        txt: "txt",
                        tags: "tags",
                        created_at: "created_at"
                    ))
                }){
                    Text("New Fastxt")
                }
            }
        }
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
    }
}

struct SearchBar: UIViewRepresentable {
    @Binding var text: String
    @Binding var notes: [Note]
    var placeholder: String
    
    class Coordinator: NSObject, UISearchBarDelegate {

        @Binding var text: String
        @Binding var notes: [Note]
        let ft = RustFastxt()

        
        init(text: Binding<String>, notes: Binding<[Note]>) {
            _text = text
            _notes = notes
        }

        func searchBar(_ searchBar: UISearchBar, textDidChange searchText: String) {
            AppState.clearOffset()
            search(input: searchText, offset: 0)
        }
        
        func search(input: String, offset: Int64){
            AppState.setQuery(query: input)
            let txt = ft.run(json_input:"""
                {"action":"search","query":"\(input)","limit":10,"offset":\(offset)}
                """
            )
            let data = txt.data(using: .utf8)!
            let decoder = JSONDecoder()
            do {
                let resp = try decoder.decode(Response.self, from: data)
                AppState.setCount(count: resp.count)
                notes = resp.notes
            } catch {
                print(error.localizedDescription)
            }
        }
    }

    func makeCoordinator() -> SearchBar.Coordinator {
        return Coordinator(text: $text, notes: $notes)
    }

    func makeUIView(context: UIViewRepresentableContext<SearchBar>) -> UISearchBar {
        let searchBar = UISearchBar(frame: .zero)
        searchBar.delegate = context.coordinator
        searchBar.placeholder = placeholder
        searchBar.searchBarStyle = .minimal
        searchBar.autocapitalizationType = .none
        return searchBar
    }

    func updateUIView(_ uiView: UISearchBar, context: UIViewRepresentableContext<SearchBar>) {
        uiView.text = text
    }
}
