//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 11/18/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var env : Env
    @State private var showingSync = false
    @State var showingCreate = false
    var body: some View {
        NavigationView {
            VStack{
                List {
                    NavigationLink(destination: CreateView(isPresented: self.$showingCreate)){
                        Text("Create").foregroundColor(.blue)
                    }
                    NavigationLink(destination: SyncView()){
                        Text("Sync").foregroundColor(.blue)
                    }
                    NavigationLink(destination: CategoryView()){
                        HStack {
                            Image(systemName: "folder.fill")
                            Text("Categories")
                        }.foregroundColor(.blue)
                    }
                    HStack{
                        TextField("type to search",text: $env.searchText)
                        Text("X").onTapGesture {
                            self.env.searchText = ""
                        }.foregroundColor(.blue)
                    }
                    HStack{
                        Text("Prev").onTapGesture {
                            let offset = AppState.decOffset()
                            AppState.search(input: AppState.getQuery(), offset: offset)
                        }.foregroundColor(.blue)
                        Spacer()
                        Text(env.paginationText)
                        Spacer()
                        Text("Next").onTapGesture {
                            let offset = AppState.incOffset()
                            AppState.search(input: AppState.getQuery(), offset: offset)
                        }.foregroundColor(.blue)

                    }
                    ForEach(env.notes, id: \.self) {
                        note in
                        NavigationLink(destination: TxtDetailView(note: note)) {
                            TxtRowView(note: note, query: self.$env.searchText)
                        }
                    }
                    //.listRowInsets(EdgeInsets())

                }
                Text("Own your txt on your device.")
            }.navigationBarTitle(Text("Fastxt"))
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
