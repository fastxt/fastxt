//
//  ServerView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/24/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ServerView: View {
    var body: some View {
        VStack{
            HStack{
                Button(action:{
                    
                }){
                    Text("Start Server")
                }
                Button(action:{
                    
                }){
                    Text("Stop Server")
                }
                Spacer()
            }
            Text("Server address")
        }
    }
}

struct ServerView_Previews: PreviewProvider {
    static var previews: some View {
        ServerView()
    }
}
