//
//  ClientView.swift
//  Fastxt
//
//  Created by Yi Wang on 4/25/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import SwiftUI

struct ClientView: View {
    var body: some View {
        VStack{
            HStack{
                Button(action:{
                    
                }){
                    Text("Start Client and Sync")
                }
                Spacer()
            }
//            TextField("address:port")
            Spacer()
        }
    }
}

struct ClientView_Previews: PreviewProvider {
    static var previews: some View {
        ClientView()
    }
}
