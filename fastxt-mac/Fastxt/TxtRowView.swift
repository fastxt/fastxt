//
//  TxtRowView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/12/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct TxtRowView: View {
    var note: Note
    @Binding var query: String
    @State private var showingAlert = false
    @State private var showingQRCode = false
    var body: some View {
        VStack(alignment: .leading){
            HStack{
                Button(action:{
                    self.showingAlert = true
                }){
                    Text("X")
                }.alert(isPresented: $showingAlert) {
                    Alert(
                        title: Text("Do you really want to delete this item \(String(note.uuid4.prefix(5))).. \(String(note.id))?"),
                        message: Text("There is no undo"),
                        primaryButton: .destructive(Text("Delete")){
                            AppState.delete(rowid: self.note.id)
                            AppState.search(input: AppState.getQuery(), offset: AppState.getOffset())
                        },
                        secondaryButton: .cancel()
                    )
                }.foregroundColor(.red).buttonStyle(BorderlessButtonStyle())
                Spacer()
                ForEach(note.tags.split(separator: ","), id:\.self){
                    tag in
                    Text(tag).onTapGesture {
                        self.query = String(tag)
                    }.foregroundColor(.blue)
                }
            }
            HStack{
                Text("\(String(note.created_at.prefix(16))) utc")
                Spacer()
                Text("\(String(note.uuid4.prefix(5))).. \(String(note.id))")
            }
            Text(makeText(note: note))
        }
    }
    func makeText(note: Note) -> String{
        return note.txt.trunc(length: 140)
    }
}

struct TxtRowView_Previews: PreviewProvider {
    static var previews: some View {
        TxtRowView(note: Note(
            id: 0,
            uuid4: "uuid4",
            txt: "txt",
            tags: "tag1,tag2",
            created_at: "2020-04-12"
        ), query: .constant("query"))
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
