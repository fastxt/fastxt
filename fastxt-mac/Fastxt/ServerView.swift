//
//  ServerView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/24/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ServerView: View {
    @Binding var isPresented: Bool
    var body: some View {
        VStack{
            HStack{
                Button(action:{
                    self.isPresented = false
                }){
                    Text("Cancel")
                }
            }
        }
    }
}

struct ServerView_Previews: PreviewProvider {
    static var previews: some View {
        ServerView(isPresented: .constant(true))
    }
}
