//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct Note: Decodable, Hashable {
    var rowid: Int64
    var uuid4: String
    var txt: String
    var tags: String
    var created_at: String
}

struct Response: Decodable {
    let count: Int64
    let notes: [Note]
}

var notes : [Note] = []

struct ContentView: View {
    @State private var searchText : String = ""

    var body: some View {
        NavigationView {
            VStack {
                SearchBar(text: $searchText, placeholder: "type to search")
                List {
                    ForEach(notes, id: \.self) {
                        note in
                        Text(note.txt)
                    }
                    TxtRowView()
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
    var placeholder: String

    class Coordinator: NSObject, UISearchBarDelegate {

        @Binding var text: String
        let ft = RustFastxt()

        
        init(text: Binding<String>) {
            _text = text
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
                print(resp.count)
            } catch {
                print(error.localizedDescription)
            }
        }
    }

    func makeCoordinator() -> SearchBar.Coordinator {
        return Coordinator(text: $text)
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
