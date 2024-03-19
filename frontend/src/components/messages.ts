export function prettyPrint(obj: string) {
	try {
		return JSON.stringify(JSON.parse(obj), null, 2);
	} catch (e) {
		return obj;
	}
}
