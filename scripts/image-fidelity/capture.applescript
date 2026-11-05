use framework "Foundation"
use scripting additions

on run argv
    set output to current application's NSMutableDictionary's dictionary()
    set openedDocument to missing value
    with timeout of 180 seconds
        tell application "Microsoft Word"
            set priorDocumentCount to count documents
            set priorActiveIdentity to missing value
            if priorDocumentCount > 0 then
                try
                    set priorActiveIdentity to posix full name of active document
                end try
            end if
            set priorSecurity to automation security
            set priorAlerts to display alerts
            set priorLinks to update links at open of settings
            set automation security to msoAutomationSecurityForceDisable
            set display alerts to alerts none
            if (count argv) > 2 then
                if item 3 of argv is "interactive" then set display alerts to alerts all
            end if
            set update links at open of settings to false
            try
                repeat with documentIndex from 1 to priorDocumentCount
                    set existingIdentity to missing value
                    try
                        set existingIdentity to posix full name of document documentIndex
                    end try
                    if existingIdentity is item 1 of argv then error "Capture target was already open; refusing ownership"
                end repeat
                open file name (item 1 of argv) read only true add to recent files false confirm conversions false
                set candidateDocument to missing value
                set identityDeadline to (current date) + 10
                repeat 50 times
                    try
                        if (count documents) > priorDocumentCount then
                            repeat with currentDocument in documents
                                if posix full name of currentDocument is item 1 of argv then
                                    set candidateDocument to currentDocument
                                    exit repeat
                                end if
                            end repeat
                        end if
                    end try
                    if candidateDocument is not missing value then exit repeat
                    if (current date) is greater than or equal to identityDeadline then exit repeat
                    delay 0.2
                end repeat
                if candidateDocument is missing value then error "Opened document identity was not established before the capture deadline"
                if posix full name of candidateDocument is not item 1 of argv then error "Opened document identity mismatch"
                if (count documents) <= priorDocumentCount then error "Opening the target did not establish new document ownership"
                set openedDocument to candidateDocument
                output's setObject:true forKey:"ownedDocumentOpened"
                output's setObject:version forKey:"wordVersion"
                output's setObject:(count shapes of openedDocument) forKey:"floatingShapeCount"
                output's setObject:(count inline shapes of openedDocument) forKey:"inlineShapeCount"
                save as openedDocument file name (item 2 of argv) file format format PDF add to recent files false
                output's setObject:"ok" forKey:"status"
            on error errorText number errorNumber
                output's setObject:"error" forKey:"status"
                output's setObject:errorText forKey:"error"
                output's setObject:errorNumber forKey:"errorCode"
            end try
            if openedDocument is not missing value then
                try
                    if posix full name of openedDocument is item 1 of argv then
                        close openedDocument saving do not save changes
                        output's setObject:true forKey:"ownedDocumentClosed"
                    else
                        output's setObject:true forKey:"cleanupSkippedIdentityMismatch"
                    end if
                end try
            end if
            set automation security to priorSecurity
            set display alerts to priorAlerts
            set update links at open of settings to priorLinks
            if priorActiveIdentity is not missing value and (count documents) > 0 then
                try
                    output's setObject:((posix full name of active document) is priorActiveIdentity) forKey:"priorActiveDocumentRestored"
                end try
            end if
            output's setObject:((count documents) is priorDocumentCount) forKey:"documentCountRestored"
        end tell
    end timeout
    set encoded to current application's NSJSONSerialization's dataWithJSONObject:output options:0 |error|:(missing value)
    return (current application's NSString's alloc()'s initWithData:encoded encoding:(current application's NSUTF8StringEncoding)) as text
end run
