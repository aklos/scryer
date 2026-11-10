/**
 * Agents pointing the view: an agent points the open app at a node over MCP, a notice offers it, and accepting it
 * moves the map or wiki there with a transition that shows where it went, then flashes what the agent wanted seen.
 * A `scryer://show` link an agent hands the developer does the same when clicked, opening the app and the project
 * first if need be.
 *
 * Beside following a session (`features/sessions`), not instead of it: following tracks where a session works,
 * mechanically, settling before each move so a burst of edits never makes the view twitch; a show is the agent
 * taking the developer somewhere to talk about it, so accepting one pauses any follow until they resume it.
 */

export { noticeFor, useDirections, type Direction, type Notice } from "./useDirections";
export { DirectionNotice } from "./DirectionNotice";
export { linkIsFor, useLinks, type LinkTarget } from "./useLinks";
export { movement, placeOf, type Move, type Place, type View } from "./movement";
export { direct, type DirectPlan, type Flash } from "./motion";
