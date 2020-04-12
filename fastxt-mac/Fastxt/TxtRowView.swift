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
    var body: some View {
        Text("\(note.id) \(note.txt) \(note.tags) \(note.created_at)")
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
        ))
    }
}
