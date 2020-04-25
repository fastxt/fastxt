//
//  TxtDetailView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/12/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct TxtDetailView: View {
    var note: Note
    var body: some View {
        VStack(alignment: .leading){
            HStack{
                Spacer()
                ForEach(note.tags.split(separator: ","), id:\.self){
                    tag in
                    Text(tag)
                }
            }
            HStack{
                Text(note.created_at.prefix(19))
                Spacer()
                Text("\(String(note.uuid4.prefix(5))).. \(String(note.id))")
            }
            Text(note.txt)
            Spacer()
        }
        .frame(minWidth: 200)
            .padding()
    }
}

struct TxtDetailView_Previews: PreviewProvider {
    static var previews: some View {
        TxtDetailView(note: Note(
            id: 0,
            uuid4: "uuid4",
            txt: "txt",
            tags: "tag1,tag2",
            created_at: "2020-04-12"
        ))
    }
}
