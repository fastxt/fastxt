//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct ContentView: View {
    @State private var searchText : String = ""
    @EnvironmentObject var env : Env
    @State private var showingSync = false
    var body: some View {
        NavigationView {
            VStack {
                HStack{
                    Text("   Fastxt")
                    SearchBar(text: $searchText, placeholder: "type to search")
                    Button(action:{
                        self.showingSync.toggle()
                    }){
                        Text("Sync   ")
                    }.sheet(isPresented: $showingSync){
                     SyncView()
                    }
                }
                HStack{
                    Button(action:{
                        let offset = AppState.decOffset()
                        AppState.search(input: AppState.getQuery(), offset: offset)
                    }){
                        Text("   Prev")
                    }//.padding()
                    Spacer()
                    Text(env.paginationText)
                    Spacer()
                    Button(action:{
                        let offset = AppState.incOffset()
                        AppState.search(input: AppState.getQuery(), offset: offset)
                    }){
                        Text("Next   ")
                    }//.padding()
                }
                List (env.notes){
                    note in
                    NavigationLink(destination: TxtDetailView(note: note)) {
                        TxtRowView(note: note, query: self.$searchText)
                    }
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
                    Text("Create New")
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

        init(text: Binding<String>) {
            _text = text
        }

        func searchBar(_ searchBar: UISearchBar, textDidChange searchText: String) {
            AppState.clearOffset()
            AppState.search(input: searchText, offset: 0)
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
