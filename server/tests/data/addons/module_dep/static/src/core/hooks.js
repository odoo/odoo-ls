/** Says a message out loud. */
export function useSpeaker() {
    return { say: (message) => message };
}

export function useSpeakerVolume() {
    return 3;
}
