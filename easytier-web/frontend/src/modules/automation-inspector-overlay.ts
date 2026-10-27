/** T33–T35: read-only Inspector graphics and gesture picks on PhoneCard's
 * original primary video. Never contains a MediaStream or ADB control token. */
export interface InspectorCardHighlight {
    left:number;top:number;right:number;bottom:number;
    sourceWidth:number;sourceHeight:number;
}
export interface InspectorCardPoint {x:number;y:number}
export interface InspectorCardDragGuide {start:InspectorCardPoint|null;end:InspectorCardPoint|null}
export interface InspectorCardScreenPoint {serial:number;x:number;y:number;width:number;height:number}
