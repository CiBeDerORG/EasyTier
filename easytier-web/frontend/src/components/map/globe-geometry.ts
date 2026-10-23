import { Curve, MathUtils, Vector3 } from 'three';

/** Equirectangular coordinates matching Three's SphereGeometry UVs. */
export function geographicPosition(latitude: number, longitude: number, radius = 1): Vector3 {
    const lat = MathUtils.degToRad(latitude);
    const lon = MathUtils.degToRad(longitude);
    return new Vector3(
        Math.cos(lat) * Math.cos(lon),
        Math.sin(lat),
        -Math.cos(lat) * Math.sin(lon),
    ).multiplyScalar(radius);
}

/** A logical network edge following the shortest great circle, above the Earth. */
export class GeographicArc extends Curve<Vector3> {
    private readonly origin: Vector3;
    private readonly axis: Vector3;
    private readonly angle: number;
    private readonly height: number;

    constructor(from: Vector3, to: Vector3) {
        super();
        this.origin = from.clone().normalize();
        const destination = to.clone().normalize();
        this.angle = Math.acos(MathUtils.clamp(this.origin.dot(destination), -1, 1));
        this.axis = new Vector3().crossVectors(this.origin, destination);
        // Antipodal points have many equally short paths. Choose a stable plane.
        if (this.axis.lengthSq() < 1e-10) {
            this.axis.crossVectors(this.origin, Math.abs(this.origin.y) < .9 ? new Vector3(0, 1, 0) : new Vector3(1, 0, 0));
        }
        this.axis.normalize();
        this.height = .045 + this.angle * .13;
    }

    override getPoint(t: number, target = new Vector3()): Vector3 {
        return target.copy(this.origin)
            .applyAxisAngle(this.axis, this.angle * t)
            .multiplyScalar(1.018 + Math.sin(Math.PI * t) * this.height);
    }
}
