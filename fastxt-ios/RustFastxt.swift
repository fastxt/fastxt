//
//  RustFastxt.swift
//  Fastxt
//
//  Created by Yi Wang on 1/21/20.
//  Copyright © 2020 Yi Wang. All rights reserved.
//

import Foundation

class RustFastxt {
    init() {
        // Keep the database in the app's Documents directory.
        if let dir = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first {
            dir.path.withCString { fastxt_set_db_dir($0) }
        }
    }

    func run(json_input: String) -> String {
        let result = fastxt_run(json_input)
        let swift_result = String(cString: result!)
        fastxt_free(UnsafeMutablePointer(mutating: result))
        return swift_result
    }
}
