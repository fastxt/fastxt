//
//  CreateView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/24/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct CreateView: View {
    @Binding var isPresented: Bool
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
                    Text("Reset")
                }
                Spacer()
                Button(action:{
                    self.isPresented = false
                }){
                    Text("Cancel")
                }
                Button(action:{
                    let input = """
                    {"action":"insert",
                    "txt":"\(String(self.txt))",
                    "tags":"\(self.tags)",
                    "limit": 10,
                    "offset": 0
                    }
                    """
                    print(input)
                    AppState.ft.run(json_input: input)
                    self.isPresented = false
                    AppState.search(input: "", offset: 0)
                }){
                    Text("Save")
                }
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
        CreateView(isPresented: .constant(true))
    }
}
