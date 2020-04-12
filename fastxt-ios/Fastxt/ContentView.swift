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

    var body: some View {
        NavigationView {
            VStack {
                SearchBar(text: $searchText, placeholder: "type to search")
                List {
                    TxtRowView()
                    TxtRowView()
                }.navigationBarTitle(Text("Fastxt"))
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
        var notes : NSArray = []
        
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
            if let jsonObject = ((try? JSONSerialization.jsonObject(with: data) as? [String: NSObject]) as [String : NSObject]??) {
                notes =  jsonObject!["notes"] as! NSArray
                //let count = jsonObject!["count"] as! Int64
                //AppState.setCount(count: count)
                print(notes)
                //paginationButton.title = AppState.makePaginationText()
            }
            
            //self.tableView.reloadData()
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
