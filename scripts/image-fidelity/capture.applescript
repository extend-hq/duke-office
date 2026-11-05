use framework "Foundation"
use scripting additions

on run argv
    set output to current application's NSMutableDictionary's dictionary()
    set openedDocument to missing value
    with timeout of 180 seconds
        tell application "Microsoft Word"
            set priorSecurity to automation security
            set priorAlerts to display alerts
            set priorLinks to update links at open of settings
            set automation security to msoAutomationSecurityForceDisable
            set display alerts to alerts none
            set update links at open of settings to false
            try
                open file name (item 1 of argv) read only true add to recent files false confirm conversions false
                set candidateDocument to active document
                if posix full name of candidateDocument is not item 1 of argv then error "Opened document identity mismatch"
                set openedDocument to candidateDocument
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
                    close openedDocument saving do not save changes
                end try
            end if
            set automation security to priorSecurity
            set display alerts to priorAlerts
            set update links at open of settings to priorLinks
        end tell
    end timeout
    set encoded to current application's NSJSONSerialization's dataWithJSONObject:output options:0 |error|:(missing value)
    return (current application's NSString's alloc()'s initWithData:encoded encoding:(current application's NSUTF8StringEncoding)) as text
end run
