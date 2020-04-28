//
//  CreateView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/24/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct CreateView: View {
    @State var tags: String = ""
    @State var txt: String = ""
    @State var textHeight: CGFloat = 150
    var body: some View {
        VStack(alignment: .leading, spacing: 20) {            
            HStack{
                Button(action:{
                    self.tags = ""
                    self.txt = ""
                }){
                    Text("Clear")
                }
                Button(action:{
                    AppState.insert(txt: self.txt, tags: self.tags)
                }){
                    Text("Save")
                }
                Spacer()
            }
            TextField("enter tags: comma or space as tag seperator", text: $tags)
            Text("enter txt below:").foregroundColor(.gray)
            MacEditorTextView(text: self.$txt)
                .frame(minWidth: 600,
                       maxWidth: .infinity,
                       minHeight: 300,
                       maxHeight: .infinity)
            Spacer()
        }.padding()
        
    }
}

struct CreateView_Previews: PreviewProvider {
    static var previews: some View {
        CreateView()
    }
}
