//
//  ContentView.swift
//  Fastxt
//
//  Created by Yi Wang on 12/6/19.
//  Copyright © 2019 Yi Wang. All rights reserved.
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject var env : Env
    @State private var showingSync = false
    @State private var showingServer = false
    
    var body: some View {
        NavigationView {
            VStack{
                HStack{
                    Text("   Fastxt")
                    Spacer()
                    NavigationLink(destination: SyncView()) {
                        Text("Sync")
                    }
                    NavigationLink(destination: CreateView()) {
                        Text("Create")
                    }
                }
                HStack{
                    TextField("type to search",text: $env.searchText)
                    Button(action:{
                        self.env.searchText = ""
                    }){
                        Text("X")
                    }
                }
                List {

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
            }.frame(minWidth: 300, maxWidth: 300)
        }.frame(minWidth: 700, minHeight: 300)
    }
}

struct ContentView_Previews: PreviewProvider {
    static var previews: some View {
        ContentView()
    }
}
